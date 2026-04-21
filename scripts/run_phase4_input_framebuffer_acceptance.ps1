# myCPU Phase 4 acceptance: framebuffer + input loop (Windows PowerShell)
#
# Goal:
#   Validate Phase 4 closed-loop behavior automatically in two modes:
#   - host-demo: fb_game + input + framebuffer loopback (default)
#   - guest-binary: guest program + stepn + framebuffer/input checks
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File .\scripts\run_phase4_input_framebuffer_acceptance.ps1
#   powershell -ExecutionPolicy Bypass -File .\scripts\run_phase4_input_framebuffer_acceptance.ps1 -Mode guest-binary

param(
    [ValidateSet('host-demo', 'guest-binary')]
    [string]$Mode = 'host-demo',

    [int]$Port = 8080,
    [int]$Warmup = 3500,
    [int]$MemoryMB = 128,

    [string]$GuestProgram,
    [string]$VirtioDisk,
    [int]$GuestStepCount = 120000,

    [string]$FramebufferAddr = '0x80E00000',
    [int]$FramebufferWidth = 320,
    [int]$FramebufferHeight = 240,
    [ValidateSet('rgb565', 'rgb888', 'gray8')]
    [string]$FramebufferFormat = 'rgb565',

    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
Set-Location $repoRoot

function Stop-ListenerOnPort {
    param([int]$TargetPort)

    $listener = Get-NetTCPConnection -LocalPort $TargetPort -State Listen -ErrorAction SilentlyContinue |
    Select-Object -First 1
    if ($null -ne $listener) {
        Write-Host "[phase4] Port $TargetPort occupied by PID $($listener.OwningProcess), terminating..." -ForegroundColor Yellow
        Stop-Process -Id $listener.OwningProcess -Force -ErrorAction SilentlyContinue
        Start-Sleep -Milliseconds 500
    }
}

function Resolve-ExistingPath {
    param([string]$PathValue, [string]$Label)

    $resolved = Resolve-Path $PathValue -ErrorAction SilentlyContinue
    if ($null -eq $resolved) {
        throw "[phase4] $Label not found: $PathValue"
    }

    return $resolved.Path
}

function New-BuiltinGuestDemoBinary {
    param([string]$OutputDir)

    New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
    $path = Join-Path $OutputDir 'phase4-linux-fb-demo-guest.bin'

    # Matches src/visualize/linux_fb_program.rs demo words.
    $hexWords = @(
        '80E002B7',
        '01F00393',
        '28000E13',
        '00729023',
        '00138393',
        '00228293',
        'FFFE0E13',
        'FE0E18E3',
        '0000006F'
    )

    $bytes = New-Object System.Collections.Generic.List[byte]
    foreach ($hexWord in $hexWords) {
        $word = [System.UInt32]::Parse($hexWord, [System.Globalization.NumberStyles]::HexNumber)
        [byte[]]$chunk = [System.BitConverter]::GetBytes($word)
        foreach ($b in $chunk) {
            $bytes.Add($b)
        }
    }

    [System.IO.File]::WriteAllBytes($path, $bytes.ToArray())
    return $path
}

function Get-FramebufferCommand {
    param([string]$CurrentMode)

    if ($CurrentMode -eq 'host-demo') {
        return 'fb linux'
    }

    return "fb $FramebufferAddr $FramebufferWidth $FramebufferHeight $FramebufferFormat"
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

    $buffer = New-Object byte[] 1048576
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

function Receive-ExpectedJson {
    param(
        [System.Net.WebSockets.ClientWebSocket]$Socket,
        [string]$ExpectedType,
        [int]$MaxMessages = 30
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

function Assert-SuccessField {
    param(
        [object]$Response,
        [string]$Context
    )

    $successProperty = $Response.PSObject.Properties['success']
    if ($null -eq $successProperty -or -not [bool]$successProperty.Value) {
        $errorProperty = $Response.PSObject.Properties['error']
        $errorText = if ($null -ne $errorProperty) { [string]$errorProperty.Value } else { 'unknown error' }
        throw "[phase4] $Context failed: $errorText"
    }
}

function Get-FrameSignature {
    param([object]$FramebufferResponse)

    $pixels = $FramebufferResponse.pixels
    if ($null -eq $pixels -or $pixels.Count -eq 0) {
        throw '[phase4] framebuffer response has empty pixels array'
    }

    [uint64]$sum = 0
    [uint64]$nonZero = 0
    foreach ($v in $pixels) {
        $value = [uint64][int]$v
        $sum = ($sum + $value) % 4294967291
        if ($value -ne 0) {
            $nonZero++
        }
    }

    return [PSCustomObject]@{
        Count   = [int]$pixels.Count
        Sum     = $sum
        NonZero = $nonZero
    }
}

if (-not $SkipBuild.IsPresent) {
    Write-Host '[phase4] Building release binary...' -ForegroundColor Cyan
    cargo build --release | Out-Null
}

$simExe = Join-Path $repoRoot 'target\release\mycpu.exe'
if (-not (Test-Path $simExe)) {
    throw "[phase4] simulator binary not found: $simExe"
}

$guestProgramAbs = $null
$virtioDiskAbs = $null
$usingBuiltinGuestDemo = $false

if ($Mode -eq 'guest-binary') {
    if ([string]::IsNullOrWhiteSpace($GuestProgram)) {
        $guestProgramAbs = New-BuiltinGuestDemoBinary -OutputDir (Join-Path $repoRoot 'target\tmp')
        $usingBuiltinGuestDemo = $true
        Write-Host "[phase4] GuestProgram not provided, generated built-in demo: $guestProgramAbs" -ForegroundColor Yellow
    }
    else {
        $guestProgramAbs = Resolve-ExistingPath -PathValue $GuestProgram -Label 'Guest program'
    }

    if (-not [string]::IsNullOrWhiteSpace($VirtioDisk)) {
        $virtioDiskAbs = Resolve-ExistingPath -PathValue $VirtioDisk -Label 'VirtIO disk image'
    }
}

# guest-binary 默认不预热：避免一次性 demo 在 baseline 采样前已进入稳态，导致 stepn 假失败。
$effectiveWarmup = $Warmup
if ($Mode -eq 'guest-binary' -and -not $PSBoundParameters.ContainsKey('Warmup')) {
    $effectiveWarmup = 0
    Write-Host '[phase4] guest-binary mode: using default warmup=0 to avoid steady-state baseline false negatives.' -ForegroundColor Yellow
}

Stop-ListenerOnPort -TargetPort $Port

$logDir = Join-Path $repoRoot 'target\phase4-acceptance-logs'
New-Item -ItemType Directory -Path $logDir -Force | Out-Null
$timestamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$backendStdout = Join-Path $logDir "phase4-$timestamp.backend.stdout.log"
$backendStderr = Join-Path $logDir "phase4-$timestamp.backend.stderr.log"
Set-Content -Path $backendStdout -Value '' -Encoding UTF8
Set-Content -Path $backendStderr -Value '' -Encoding UTF8

$backendArgs = @(
    'visualize',
    '--port', $Port.ToString(),
    '--memory', $MemoryMB.ToString(),
    '--warmup', $effectiveWarmup.ToString()
)

if ($Mode -eq 'host-demo') {
    $backendArgs += '--linux-fb-demo'
}
else {
    $backendArgs += $guestProgramAbs
    if (-not [string]::IsNullOrWhiteSpace($virtioDiskAbs)) {
        $backendArgs += @('--virtio-disk', $virtioDiskAbs)
    }
}

Write-Host "[phase4] Starting backend on ws://127.0.0.1:$Port ..." -ForegroundColor Cyan
$backend = Start-Process -FilePath $simExe `
    -ArgumentList $backendArgs `
    -WorkingDirectory $repoRoot `
    -RedirectStandardOutput $backendStdout `
    -RedirectStandardError $backendStderr `
    -PassThru

$socket = [System.Net.WebSockets.ClientWebSocket]::new()

try {
    $connected = $false
    for ($retry = 1; $retry -le 60; $retry++) {
        if ($backend.HasExited) {
            $outTail = if (Test-Path $backendStdout) { (Get-Content $backendStdout | Select-Object -Last 40) -join "`n" } else { '' }
            $errTail = if (Test-Path $backendStderr) { (Get-Content $backendStderr | Select-Object -Last 40) -join "`n" } else { '' }
            throw "[phase4] backend exited early with code $($backend.ExitCode).`nSTDOUT:`n$outTail`nSTDERR:`n$errTail"
        }

        try {
            $socket.ConnectAsync(
                [Uri]("ws://127.0.0.1:{0}" -f $Port),
                [System.Threading.CancellationToken]::None
            ).GetAwaiter().GetResult() | Out-Null
            $connected = $true
            break
        }
        catch {
            Start-Sleep -Milliseconds 500
        }
    }

    if (-not $connected) {
        throw '[phase4] failed to connect websocket to backend'
    }

    # Consume initial snapshot message.
    $null = Receive-WebSocketText -Socket $socket

    $frameCmd = Get-FramebufferCommand -CurrentMode $Mode

    if ($Mode -eq 'host-demo') {
        Send-WebSocketText -Socket $socket -Text 'fb_game init'
        $gameInit = Receive-ExpectedJson -Socket $socket -ExpectedType 'framebuffer_game'
        Assert-SuccessField -Response $gameInit -Context 'fb_game init'

        Send-WebSocketText -Socket $socket -Text $frameCmd
        $frame0 = Receive-ExpectedJson -Socket $socket -ExpectedType 'framebuffer'
        Assert-SuccessField -Response $frame0 -Context 'framebuffer read before input'
        $sig0 = Get-FrameSignature -FramebufferResponse $frame0

        Send-WebSocketText -Socket $socket -Text 'input right down'
        $inputDown = Receive-ExpectedJson -Socket $socket -ExpectedType 'input_ack'
        Assert-SuccessField -Response $inputDown -Context 'input right down'

        Send-WebSocketText -Socket $socket -Text 'input state'
        $inputStateDown = Receive-ExpectedJson -Socket $socket -ExpectedType 'input_state'
        Assert-SuccessField -Response $inputStateDown -Context 'input state after right down'
        if (([int]$inputStateDown.key_state -band 8) -eq 0) {
            throw '[phase4] expected key_state bit for RIGHT key to be set after right down'
        }

        for ($i = 0; $i -lt 6; $i++) {
            Send-WebSocketText -Socket $socket -Text 'fb_game step'
            $stepResp = Receive-ExpectedJson -Socket $socket -ExpectedType 'framebuffer_game'
            Assert-SuccessField -Response $stepResp -Context ("fb_game step #$($i + 1)")
        }

        Send-WebSocketText -Socket $socket -Text 'fb_game state'
        $gameState = Receive-ExpectedJson -Socket $socket -ExpectedType 'framebuffer_game'
        Assert-SuccessField -Response $gameState -Context 'fb_game state after stepping'
        if (($null -eq $gameState.PSObject.Properties['tick']) -or ([int]$gameState.tick -lt 6)) {
            throw "[phase4] expected fb_game tick >= 6 after stepping, got $([int]$gameState.tick)"
        }
        if (($null -eq $gameState.PSObject.Properties['ball_x']) -or ($null -eq $gameState.PSObject.Properties['ball_y'])) {
            throw '[phase4] expected fb_game state to include ball_x/ball_y coordinates'
        }

        Send-WebSocketText -Socket $socket -Text $frameCmd
        $frame1 = Receive-ExpectedJson -Socket $socket -ExpectedType 'framebuffer'
        Assert-SuccessField -Response $frame1 -Context 'framebuffer read after steps'
        $sig1 = Get-FrameSignature -FramebufferResponse $frame1

        if ($sig0.NonZero -le 0 -or $sig1.NonZero -le 0) {
            throw '[phase4] framebuffer should contain non-zero pixels before and after stepping'
        }

        if ($sig0.Sum -eq $sig1.Sum) {
            throw '[phase4] framebuffer signature did not change after input + game steps'
        }

        Send-WebSocketText -Socket $socket -Text 'input right up'
        $inputUp = Receive-ExpectedJson -Socket $socket -ExpectedType 'input_ack'
        Assert-SuccessField -Response $inputUp -Context 'input right up'

        Send-WebSocketText -Socket $socket -Text 'input clear'
        $inputClear = Receive-ExpectedJson -Socket $socket -ExpectedType 'input_ack'
        Assert-SuccessField -Response $inputClear -Context 'input clear'

        Send-WebSocketText -Socket $socket -Text 'input state'
        $inputStateClear = Receive-ExpectedJson -Socket $socket -ExpectedType 'input_state'
        Assert-SuccessField -Response $inputStateClear -Context 'input state after clear'
        if ([int]$inputStateClear.key_state -ne 0) {
            throw "[phase4] expected key_state=0 after clear, got $([int]$inputStateClear.key_state)"
        }

        Write-Host "[phase4] PASS(host-demo): frame changed ($($sig0.Sum) -> $($sig1.Sum)), nonZero=$($sig1.NonZero), input loop verified." -ForegroundColor Green
    }
    else {
        Send-WebSocketText -Socket $socket -Text 'input clear'
        $inputClearBefore = Receive-ExpectedJson -Socket $socket -ExpectedType 'input_ack'
        Assert-SuccessField -Response $inputClearBefore -Context 'input clear before guest stepping'

        Send-WebSocketText -Socket $socket -Text $frameCmd
        $frame0 = Receive-ExpectedJson -Socket $socket -ExpectedType 'framebuffer'
        Assert-SuccessField -Response $frame0 -Context 'guest framebuffer baseline'
        $sig0 = Get-FrameSignature -FramebufferResponse $frame0

        Send-WebSocketText -Socket $socket -Text ("stepn {0}" -f $GuestStepCount)
        $stepN = Receive-ExpectedJson -Socket $socket -ExpectedType 'stepn'
        Assert-SuccessField -Response $stepN -Context 'guest stepn advance'
        $executedProp = $stepN.PSObject.Properties['executed']
        if ($null -eq $executedProp -or [int]$executedProp.Value -le 0) {
            throw "[phase4] expected stepn executed > 0, got $($executedProp.Value)"
        }

        Send-WebSocketText -Socket $socket -Text $frameCmd
        $frame1 = Receive-ExpectedJson -Socket $socket -ExpectedType 'framebuffer'
        Assert-SuccessField -Response $frame1 -Context 'guest framebuffer after stepn'
        $sig1 = Get-FrameSignature -FramebufferResponse $frame1

        if ($sig1.NonZero -le 0) {
            throw '[phase4] guest framebuffer still fully zero after stepn advance'
        }

        if ($sig0.Sum -eq $sig1.Sum) {
            if ($usingBuiltinGuestDemo -and $sig0.NonZero -gt 0 -and $sig1.NonZero -gt 0) {
                Write-Host '[phase4] guest framebuffer signature unchanged after stepn (built-in finite demo likely reached steady-state); accepting non-zero framebuffer as pass.' -ForegroundColor Yellow
            }
            else {
                throw '[phase4] guest framebuffer signature did not change after stepn advance'
            }
        }

        $frameOutcome = if ($sig0.Sum -eq $sig1.Sum) {
            "frame stable ($($sig0.Sum) -> $($sig1.Sum))"
        }
        else {
            "frame changed ($($sig0.Sum) -> $($sig1.Sum))"
        }

        Send-WebSocketText -Socket $socket -Text 'input right down'
        $inputDown = Receive-ExpectedJson -Socket $socket -ExpectedType 'input_ack'
        Assert-SuccessField -Response $inputDown -Context 'guest input right down'

        Send-WebSocketText -Socket $socket -Text 'input state'
        $inputStateDown = Receive-ExpectedJson -Socket $socket -ExpectedType 'input_state'
        Assert-SuccessField -Response $inputStateDown -Context 'guest input state after right down'
        if (([int]$inputStateDown.key_state -band 8) -eq 0) {
            throw '[phase4] expected key_state bit for RIGHT key to be set in guest mode'
        }

        Send-WebSocketText -Socket $socket -Text 'input right up'
        $inputUp = Receive-ExpectedJson -Socket $socket -ExpectedType 'input_ack'
        Assert-SuccessField -Response $inputUp -Context 'guest input right up'

        Send-WebSocketText -Socket $socket -Text 'input clear'
        $inputClear = Receive-ExpectedJson -Socket $socket -ExpectedType 'input_ack'
        Assert-SuccessField -Response $inputClear -Context 'guest input clear'

        Send-WebSocketText -Socket $socket -Text 'input state'
        $inputStateClear = Receive-ExpectedJson -Socket $socket -ExpectedType 'input_state'
        Assert-SuccessField -Response $inputStateClear -Context 'guest input state after clear'
        if ([int]$inputStateClear.key_state -ne 0) {
            throw "[phase4] expected key_state=0 after clear in guest mode, got $([int]$inputStateClear.key_state)"
        }

        Write-Host "[phase4] PASS(guest-binary): $frameOutcome, nonZero=$($sig1.NonZero), stepn=$GuestStepCount, input loop verified." -ForegroundColor Green
    }
}
finally {
    if ($socket.State -eq [System.Net.WebSockets.WebSocketState]::Open) {
        $socket.Abort()
    }
    $socket.Dispose()

    if ($null -ne $backend -and -not $backend.HasExited) {
        Stop-Process -Id $backend.Id -Force -ErrorAction SilentlyContinue
    }

    Write-Host "[phase4] Backend logs: $backendStdout / $backendStderr" -ForegroundColor DarkCyan
}
