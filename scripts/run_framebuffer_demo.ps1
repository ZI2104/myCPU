# myCPU Framebuffer Demo Launcher + Phase6 Acceptance (Windows PowerShell)
# Usage:
#   powershell -ExecutionPolicy Bypass -File .\scripts\run_framebuffer_demo.ps1
#   powershell -ExecutionPolicy Bypass -File .\scripts\run_framebuffer_demo.ps1 -Mode pattern-demo

param(
    [ValidateSet('linux-program', 'pattern-demo')]
    [string]$Mode = 'linux-program',

    [int]$Warmup = 0,

    [switch]$SkipProbe
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
$frontendRoot = Join-Path $repoRoot 'frontend'

function Stop-ListenerOnPort {
    param(
        [int]$Port,
        [string]$Label
    )

    $listener = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue |
    Select-Object -First 1

    if ($null -ne $listener) {
        Write-Host "[myCPU] Port $Port is occupied by PID $($listener.OwningProcess), stopping it for $Label..." -ForegroundColor Yellow
        Stop-Process -Id $listener.OwningProcess -Force -ErrorAction SilentlyContinue
        Start-Sleep -Milliseconds 500
    }
}

function Send-WebSocketText {
    param(
        [System.Net.WebSockets.ClientWebSocket]$Socket,
        [string]$Text
    )

    $bytes = [System.Text.Encoding]::UTF8.GetBytes($Text)
    $segment = [System.ArraySegment[byte]]::new($bytes)
    $Socket.SendAsync(
        $segment,
        [System.Net.WebSockets.WebSocketMessageType]::Text,
        $true,
        [System.Threading.CancellationToken]::None
    ).GetAwaiter().GetResult() | Out-Null
}

function Receive-WebSocketText {
    param([System.Net.WebSockets.ClientWebSocket]$Socket)

    $buffer = New-Object byte[] 65536
    $ms = New-Object System.IO.MemoryStream

    while ($true) {
        $segment = [System.ArraySegment[byte]]::new($buffer)
        $result = $Socket.ReceiveAsync(
            $segment,
            [System.Threading.CancellationToken]::None
        ).GetAwaiter().GetResult()

        if ($result.MessageType -eq [System.Net.WebSockets.WebSocketMessageType]::Close) {
            throw 'WebSocket closed before a response was received.'
        }

        $ms.Write($buffer, 0, $result.Count)
        if ($result.EndOfMessage) {
            break
        }
    }

    return [System.Text.Encoding]::UTF8.GetString($ms.ToArray())
}

function Receive-ExpectedWebSocketJson {
    param(
        [System.Net.WebSockets.ClientWebSocket]$Socket,
        [string]$ExpectedType,
        [int]$MaxMessages = 20
    )

    for ($i = 1; $i -le $MaxMessages; $i++) {
        $text = Receive-WebSocketText -Socket $Socket
        $obj = $text | ConvertFrom-Json

        if ([string]::IsNullOrWhiteSpace($ExpectedType)) {
            return $obj
        }

        $typeProperty = $obj.PSObject.Properties['type']
        if ($null -ne $typeProperty -and $typeProperty.Value -eq $ExpectedType) {
            return $obj
        }
    }

    throw "Did not receive expected response type '$ExpectedType' within $MaxMessages messages."
}

function Invoke-FramebufferProbe {
    param(
        [string[]]$Commands,
        [System.Diagnostics.Process]$BackendProcess,
        [string]$BackendStdout,
        [string]$BackendStderr,
        [int]$MaxRetries = 120
    )

    for ($retry = 1; $retry -le $MaxRetries; $retry++) {
        $socket = [System.Net.WebSockets.ClientWebSocket]::new()

        try {
            if ($BackendProcess.HasExited) {
                $stdoutTail = ''
                $stderrTail = ''
                if (Test-Path $BackendStdout) {
                    $stdoutTail = (Get-Content $BackendStdout | Select-Object -Last 30) -join "`n"
                }
                if (Test-Path $BackendStderr) {
                    $stderrTail = (Get-Content $BackendStderr | Select-Object -Last 30) -join "`n"
                }
                throw "Visualization backend exited early (code=$($BackendProcess.ExitCode)).`nSTDOUT:`n$stdoutTail`nSTDERR:`n$stderrTail"
            }

            $socket.ConnectAsync(
                [Uri]'ws://127.0.0.1:8080',
                [System.Threading.CancellationToken]::None
            ).GetAwaiter().GetResult() | Out-Null

            $response = $null
            foreach ($command in $Commands) {
                Send-WebSocketText -Socket $socket -Text $command

                $expectedType = if ($command.StartsWith('fb_demo')) {
                    'framebuffer_demo'
                }
                elseif ($command.StartsWith('fb')) {
                    'framebuffer'
                }
                else {
                    ''
                }

                $response = Receive-ExpectedWebSocketJson `
                    -Socket $socket `
                    -ExpectedType $expectedType
            }

            if ($null -eq $response) {
                throw 'Probe did not receive any response payload.'
            }
            $successProperty = $response.PSObject.Properties['success']
            if ($null -eq $successProperty -or -not [bool]$successProperty.Value) {
                $errorProperty = $response.PSObject.Properties['error']
                $errorText = if ($null -ne $errorProperty) { $errorProperty.Value } else { 'unknown error' }
                throw "Probe failed: $errorText"
            }

            $hasNonZero = $false
            $pixelsProperty = $response.PSObject.Properties['pixels']
            if ($null -eq $pixelsProperty) {
                throw 'Probe response does not contain pixels field.'
            }

            foreach ($value in $pixelsProperty.Value) {
                if ([int]$value -ne 0) {
                    $hasNonZero = $true
                    break
                }
            }

            if (-not $hasNonZero) {
                throw 'Framebuffer pixels are all zero; demo output not visible yet.'
            }

            return
        }
        catch {
            if ($retry -ge $MaxRetries) {
                throw
            }
            Start-Sleep -Milliseconds 1000
        }
        finally {
            if ($socket.State -eq [System.Net.WebSockets.WebSocketState]::Open) {
                $socket.Abort()
            }
            $socket.Dispose()
        }
    }
}

$backendArgs = if ($Mode -eq 'linux-program') {
    @('run', '--', 'visualize', '--port', '8080', '--linux-fb-demo', '--warmup', $Warmup.ToString())
}
else {
    @('run', '--', 'visualize', '--port', '8080')
}

Write-Host '[myCPU] Starting visualization backend (ws://127.0.0.1:8080)...' -ForegroundColor Cyan
Stop-ListenerOnPort -Port 8080 -Label 'visualization backend'
$logDir = Join-Path $repoRoot 'target\phase6-demo-logs'
New-Item -ItemType Directory -Path $logDir -Force | Out-Null
$backendStdout = Join-Path $logDir 'backend.stdout.log'
$backendStderr = Join-Path $logDir 'backend.stderr.log'
Set-Content -Path $backendStdout -Value '' -Encoding UTF8
Set-Content -Path $backendStderr -Value '' -Encoding UTF8
$backend = Start-Process -FilePath 'cargo' `
    -ArgumentList $backendArgs `
    -WorkingDirectory $repoRoot `
    -RedirectStandardOutput $backendStdout `
    -RedirectStandardError $backendStderr `
    -PassThru

Write-Host '[myCPU] Probing framebuffer output for acceptance...' -ForegroundColor Cyan
if (-not $SkipProbe) {
    if ($Mode -eq 'linux-program') {
        Invoke-FramebufferProbe `
            -Commands @('fb linux') `
            -BackendProcess $backend `
            -BackendStdout $backendStdout `
            -BackendStderr $backendStderr
    }
    else {
        Invoke-FramebufferProbe `
            -Commands @('fb_demo gradient', 'fb linux') `
            -BackendProcess $backend `
            -BackendStdout $backendStdout `
            -BackendStderr $backendStderr
    }
    Write-Host '[myCPU] Framebuffer probe passed (non-zero pixels observed).' -ForegroundColor Green
}
else {
    Write-Host '[myCPU] Probe skipped by -SkipProbe.' -ForegroundColor Yellow
}

Write-Host '[myCPU] Starting frontend dev server (http://127.0.0.1:5173)...' -ForegroundColor Cyan
Stop-ListenerOnPort -Port 5173 -Label 'frontend dev server'
$frontend = Start-Process -FilePath 'npm.cmd' `
    -ArgumentList @('run', 'dev', '--', '--host', '127.0.0.1', '--port', '5173') `
    -WorkingDirectory $frontendRoot `
    -PassThru

Start-Sleep -Seconds 3
Start-Process 'http://127.0.0.1:5173'

Write-Host ''
Write-Host 'Demo started. In browser:' -ForegroundColor Green
Write-Host '  1) Open "Framebuffer" tab' -ForegroundColor Green
if ($Mode -eq 'linux-program') {
    Write-Host '  2) Click "Linux Preset"' -ForegroundColor Green
    Write-Host '  3) Click "Refresh" (built-in RV32I program should already draw)' -ForegroundColor Green
}
else {
    Write-Host '  2) Click "Linux Preset"' -ForegroundColor Green
    Write-Host '  3) Select pattern and click "Demo Frame"' -ForegroundColor Green
}
Write-Host ''
Write-Host 'Press Ctrl+C to stop all demo processes.' -ForegroundColor Yellow

try {
    while ($true) {
        Start-Sleep -Seconds 1
        if ($backend.HasExited) {
            Write-Warning "Visualization backend exited with code $($backend.ExitCode)."
            break
        }
        if ($frontend.HasExited) {
            Write-Warning "Frontend dev server exited with code $($frontend.ExitCode)."
            break
        }
    }
}
finally {
    if (-not $backend.HasExited) {
        Stop-Process -Id $backend.Id -Force
    }
    if (-not $frontend.HasExited) {
        Stop-Process -Id $frontend.Id -Force
    }
    Write-Host '[myCPU] Demo processes stopped.' -ForegroundColor Cyan
}
