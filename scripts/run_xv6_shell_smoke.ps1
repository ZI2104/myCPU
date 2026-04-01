# myCPU xv6 shell smoke (Windows PowerShell)
# Usage:
#   powershell -ExecutionPolicy Bypass -File .\scripts\run_xv6_shell_smoke.ps1
#   powershell -ExecutionPolicy Bypass -File .\scripts\run_xv6_shell_smoke.ps1 -Mode matrix
#   powershell -ExecutionPolicy Bypass -File .\scripts\run_xv6_shell_smoke.ps1 -Mode matrix -MatrixScenarios echo,ls,grep,wc

param(
    [ValidateSet('single', 'matrix')]
    [string]$Mode = 'single',

    [ValidateSet('echo', 'ls', 'cat', 'grep', 'wc', 'usertests')]
    [string[]]$MatrixScenarios = @('echo', 'ls', 'cat', 'grep', 'wc'),

    [int]$Count = 200000000,
    [int]$HeartbeatEvery = 0,
    [ValidateSet('compact', 'diagnostic')]
    [string]$HeartbeatMode = 'compact',
    [ValidateSet('step', 'prompt')]
    [string]$UartInjectTrigger = 'prompt',
    [int]$MemoryMB = 128,
    [string]$KernelPath = "third_party/xv6-rv32/kernel/kernel",
    [string]$DiskPath = "third_party/xv6-rv32/fs.img",
    [string]$UartScript = "echo HI\n",
    [long]$UartInjectAt = 130000000,
    [int]$UartInjectEvery = 5000
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
Set-Location $repoRoot

$kernelAbs = Join-Path $repoRoot $KernelPath
$diskAbs = Join-Path $repoRoot $DiskPath

if (-not (Test-Path $kernelAbs)) {
    throw "Kernel file not found: $kernelAbs"
}
if (-not (Test-Path $diskAbs)) {
    throw "Disk image not found: $diskAbs"
}

Write-Host '[shell-smoke] Building release binary once...' -ForegroundColor Cyan
$buildStdout = Join-Path $env:TEMP 'mycpu-shell-smoke-build.stdout.log'
$buildStderr = Join-Path $env:TEMP 'mycpu-shell-smoke-build.stderr.log'
$buildProc = Start-Process -FilePath 'cargo' `
    -ArgumentList @('build', '--release') `
    -WorkingDirectory $repoRoot `
    -RedirectStandardOutput $buildStdout `
    -RedirectStandardError $buildStderr `
    -PassThru `
    -Wait

if ($buildProc.ExitCode -ne 0) {
    $buildOut = if (Test-Path $buildStdout) { Get-Content $buildStdout -Raw } else { '' }
    $buildErr = if (Test-Path $buildStderr) { Get-Content $buildStderr -Raw } else { '' }
    throw "[shell-smoke] build failed with exit code $($buildProc.ExitCode)`n$buildOut`n$buildErr"
}

$simExe = Join-Path $repoRoot 'target\release\mycpu.exe'
if (-not (Test-Path $simExe)) {
    throw "[shell-smoke] simulator binary not found after build: $simExe"
}

$logDir = Join-Path $repoRoot 'target\shell-smoke-logs'
New-Item -ItemType Directory -Path $logDir -Force | Out-Null

function Test-RequiredPatterns {
    param(
        [string]$Content,
        [string[]]$RequiredPatterns
    )

    $missing = @()
    foreach ($pattern in $RequiredPatterns) {
        if (-not [System.Text.RegularExpressions.Regex]::IsMatch($Content, $pattern)) {
            $missing += $pattern
        }
    }
    return $missing
}

function Test-OrderedMarkers {
    param(
        [string]$Content,
        [string[]]$OrderedMarkers
    )

    $missing = @()
    $cursor = 0

    foreach ($marker in $OrderedMarkers) {
        $index = $Content.IndexOf($marker, $cursor, [System.StringComparison]::Ordinal)
        if ($index -lt 0) {
            $missing += "ordered marker: $marker"
            continue
        }
        $cursor = $index + $marker.Length
    }

    return $missing
}

function Test-SessionStateMachine {
    param(
        [string]$Content,
        [object[]]$SessionSteps
    )

    if ($null -eq $SessionSteps -or $SessionSteps.Count -eq 0) {
        return @()
    }

    $lines = $Content -split "`r?`n"
    $stepIndex = 0

    for ($i = 0; $i -lt $lines.Count -and $stepIndex -lt $SessionSteps.Count; $i++) {
        while ($stepIndex -lt $SessionSteps.Count) {
            $step = $SessionSteps[$stepIndex]
            $stepPattern = [string]$step.Pattern

            if ([string]::IsNullOrWhiteSpace($stepPattern)) {
                $stepIndex++
                continue
            }

            if (-not [System.Text.RegularExpressions.Regex]::IsMatch($lines[$i], $stepPattern)) {
                break
            }

            $stepIndex++
        }
    }

    $missing = @()
    for ($j = $stepIndex; $j -lt $SessionSteps.Count; $j++) {
        $step = $SessionSteps[$j]
        $stepName = if ($null -ne $step.Name -and -not [string]::IsNullOrWhiteSpace([string]$step.Name)) {
            [string]$step.Name
        }
        else {
            "step_$($j + 1)"
        }
        $missing += "session state: $stepName"
    }

    return $missing
}

function Get-FailureCategory {
    param([string[]]$MissingItems)

    foreach ($item in $MissingItems) {
        if ($item -match 'cargo exit code|sim exit code|init: starting sh') {
            return 'startup'
        }
    }

    foreach ($item in $MissingItems) {
        if ($item -match 'UART script injected|ordered marker') {
            return 'injection'
        }
    }

    return 'assertion'
}

function Invoke-SmokeCase {
    param(
        [string]$CaseName,
        [string]$CaseUartScript,
        [string[]]$RequiredPatterns,
        [string[]]$OrderedMarkers = @(),
        [object[]]$SessionSteps = @()
    )

    $timestamp = Get-Date -Format 'yyyyMMdd-HHmmss'
    $safeCase = $CaseName -replace '[^a-zA-Z0-9_-]', '_'
    $logFile = Join-Path $logDir "shell-smoke-$timestamp-$safeCase.log"
    $stdoutLog = Join-Path $logDir "shell-smoke-$timestamp-$safeCase.stdout.log"
    $stderrLog = Join-Path $logDir "shell-smoke-$timestamp-$safeCase.stderr.log"
    $uartScriptArg = '"' + ($CaseUartScript -replace '"', '\\"') + '"'

    Write-Host "[shell-smoke][$CaseName] Running..." -ForegroundColor Cyan
    Write-Host "[shell-smoke][$CaseName] Log: $logFile" -ForegroundColor Cyan

    $effectiveInjectAt = if ($UartInjectTrigger -eq 'prompt') { 0 } else { $UartInjectAt }
    $effectiveInjectEvery = if ($UartInjectTrigger -eq 'prompt') { 1 } else { [Math]::Max($UartInjectEvery, 1) }

    $cmdArgs = @(
        'run',
        '--count', $Count,
        '--heartbeat-every', $HeartbeatEvery,
        '--heartbeat-mode', $HeartbeatMode,
        '--memory', $MemoryMB,
        $kernelAbs,
        '--virtio-disk', $diskAbs,
        '--uart-inject-trigger', $UartInjectTrigger,
        '--uart-script', $uartScriptArg,
        '--uart-inject-at', $effectiveInjectAt,
        '--uart-inject-every', $effectiveInjectEvery
    )

    $proc = Start-Process -FilePath $simExe `
        -ArgumentList $cmdArgs `
        -WorkingDirectory $repoRoot `
        -RedirectStandardOutput $stdoutLog `
        -RedirectStandardError $stderrLog `
        -PassThru `
        -Wait

    $stdoutText = if (Test-Path $stdoutLog) { Get-Content $stdoutLog -Raw } else { '' }
    $stderrText = if (Test-Path $stderrLog) { Get-Content $stderrLog -Raw } else { '' }
    $content = @($stdoutText, $stderrText) -join "`n"
    Set-Content -Path $logFile -Value $content -Encoding UTF8

    Write-Host "[shell-smoke][$CaseName] --- output tail (last 40 lines) ---" -ForegroundColor DarkGray
    ($content -split "`r?`n" | Select-Object -Last 40) | ForEach-Object { Write-Host $_ }

    if ($null -ne $proc.ExitCode -and $proc.ExitCode -ne 0) {
        return [PSCustomObject]@{
            Name            = $CaseName
            Passed          = $false
            LogFile         = $logFile
            MissingPatterns = @("sim exit code $($proc.ExitCode)")
            FailureCategory = 'startup'
        }
    }

    $missing = @(Test-RequiredPatterns -Content $content -RequiredPatterns $RequiredPatterns)
    $orderedMissing = @(Test-OrderedMarkers -Content $content -OrderedMarkers $OrderedMarkers)
    $stateMissing = @(Test-SessionStateMachine -Content $content -SessionSteps $SessionSteps)
    $allMissing = @($missing + $orderedMissing + $stateMissing)

    return [PSCustomObject]@{
        Name            = $CaseName
        Passed          = ($allMissing.Count -eq 0)
        LogFile         = $logFile
        MissingPatterns = @($allMissing)
        FailureCategory = if ($allMissing.Count -eq 0) {
            ''
        }
        else {
            Get-FailureCategory -MissingItems $allMissing
        }
    }
}

function Get-MatrixCase {
    param([string]$Scenario)

    switch ($Scenario) {
        'echo' {
            return @{
                Name             = 'echo'
                UartScript       = "echo HI\n"
                RequiredPatterns = @(
                    '(?m)init: starting sh',
                    '(?m)^(?:\$\s*)?echo HI$',
                    '(?m)^HI$',
                    '(?m)\$ ',
                    '(?m)UART script injected:'
                )
                OrderedMarkers   = @(
                    'echo HI',
                    'HI',
                    '$ '
                )
                SessionSteps     = @(
                    @{ Name = 'shell_start'; Pattern = '^init: starting sh$' },
                    @{ Name = 'prompt_ready'; Pattern = '^\$\s?' },
                    @{ Name = 'cmd_echo'; Pattern = '^(?:\$\s*)?echo HI$' },
                    @{ Name = 'cmd_output'; Pattern = '^HI$' },
                    @{ Name = 'prompt_return'; Pattern = '^\$\s?' }
                )
            }
        }
        'ls' {
            return @{
                Name             = 'ls'
                UartScript       = "ls\necho LS_DONE\n"
                RequiredPatterns = @(
                    '(?m)init: starting sh',
                    '(?m)^(?:\$\s*)?ls$',
                    '(?m)^(?:\$\s*)?echo LS_DONE$',
                    '(?m)^(?:\$\s*)?LS_DONE$',
                    '(?m)\$ ',
                    '(?m)UART script injected:'
                )
                OrderedMarkers   = @(
                    'ls',
                    'echo LS_DONE',
                    'LS_DONE'
                )
                SessionSteps     = @(
                    @{ Name = 'shell_start'; Pattern = '^init: starting sh$' },
                    @{ Name = 'prompt_ready'; Pattern = '^\$\s?' },
                    @{ Name = 'cmd_ls'; Pattern = '^(?:\$\s*)?ls$' },
                    @{ Name = 'cmd_done_echo'; Pattern = '^(?:\$\s*)?echo LS_DONE$' },
                    @{ Name = 'done_seen'; Pattern = '^(?:\$\s*)?LS_DONE$' },
                    @{ Name = 'prompt_return'; Pattern = '^\$\s?' }
                )
            }
        }
        'cat' {
            return @{
                Name             = 'cat'
                UartScript       = "cat README\necho CAT_DONE\n"
                RequiredPatterns = @(
                    '(?m)init: starting sh',
                    '(?m)^(?:\$\s*)?cat README$',
                    '(?m)^(?:\$\s*)?echo CAT_DONE$',
                    '(?m)^(?:\$\s*)?CAT_DONE$',
                    '(?m)\$ ',
                    '(?m)UART script injected:'
                )
                OrderedMarkers   = @(
                    'cat README',
                    'echo CAT_DONE',
                    'CAT_DONE'
                )
                SessionSteps     = @(
                    @{ Name = 'shell_start'; Pattern = '^init: starting sh$' },
                    @{ Name = 'prompt_ready'; Pattern = '^\$\s?' },
                    @{ Name = 'cmd_cat'; Pattern = '^(?:\$\s*)?cat README$' },
                    @{ Name = 'cmd_done_echo'; Pattern = '^(?:\$\s*)?echo CAT_DONE$' },
                    @{ Name = 'done_seen'; Pattern = '^(?:\$\s*)?CAT_DONE$' },
                    @{ Name = 'prompt_return'; Pattern = '^\$\s?' }
                )
            }
        }
        'grep' {
            return @{
                Name             = 'grep'
                UartScript       = "grep xv6 README\necho GREP_DONE\n"
                RequiredPatterns = @(
                    '(?m)init: starting sh',
                    '(?m)^(?:\$\s*)?grep xv6 README$',
                    '(?m)^(?:\$\s*)?echo GREP_DONE$',
                    '(?m)^(?:\$\s*)?GREP_DONE$',
                    '(?m)\$ ',
                    '(?m)UART script injected:'
                )
                OrderedMarkers   = @(
                    'grep xv6 README',
                    'echo GREP_DONE',
                    'GREP_DONE'
                )
                SessionSteps     = @(
                    @{ Name = 'shell_start'; Pattern = '^init: starting sh$' },
                    @{ Name = 'prompt_ready'; Pattern = '^\$\s?' },
                    @{ Name = 'cmd_grep'; Pattern = '^(?:\$\s*)?grep xv6 README$' },
                    @{ Name = 'cmd_done_echo'; Pattern = '^(?:\$\s*)?echo GREP_DONE$' },
                    @{ Name = 'done_seen'; Pattern = '^(?:\$\s*)?GREP_DONE$' },
                    @{ Name = 'prompt_return'; Pattern = '^\$\s?' }
                )
            }
        }
        'wc' {
            return @{
                Name             = 'wc'
                UartScript       = "wc README\necho WC_DONE\n"
                RequiredPatterns = @(
                    '(?m)init: starting sh',
                    '(?m)^(?:\$\s*)?wc README$',
                    '(?m)^(?:\$\s*)?echo WC_DONE$',
                    '(?m)^(?:\$\s*)?WC_DONE$',
                    '(?m)\$ ',
                    '(?m)UART script injected:'
                )
                OrderedMarkers   = @(
                    'wc README',
                    'echo WC_DONE',
                    'WC_DONE'
                )
                SessionSteps     = @(
                    @{ Name = 'shell_start'; Pattern = '^init: starting sh$' },
                    @{ Name = 'prompt_ready'; Pattern = '^\$\s?' },
                    @{ Name = 'cmd_wc'; Pattern = '^(?:\$\s*)?wc README$' },
                    @{ Name = 'cmd_done_echo'; Pattern = '^(?:\$\s*)?echo WC_DONE$' },
                    @{ Name = 'done_seen'; Pattern = '^(?:\$\s*)?WC_DONE$' },
                    @{ Name = 'prompt_return'; Pattern = '^\$\s?' }
                )
            }
        }
        'usertests' {
            return @{
                Name             = 'usertests'
                UartScript       = "usertests\necho USERTESTS_DONE\n"
                RequiredPatterns = @(
                    '(?m)init: starting sh',
                    '(?m)^(?:\$\s*)?usertests$',
                    '(?m)^(?:\$\s*)?echo USERTESTS_DONE$',
                    '(?m)^(?:\$\s*)?USERTESTS_DONE$',
                    '(?m)\$ ',
                    '(?m)UART script injected:'
                )
                OrderedMarkers   = @(
                    'usertests',
                    'echo USERTESTS_DONE',
                    'USERTESTS_DONE'
                )
                SessionSteps     = @(
                    @{ Name = 'shell_start'; Pattern = '^init: starting sh$' },
                    @{ Name = 'prompt_ready'; Pattern = '^\$\s?' },
                    @{ Name = 'cmd_usertests'; Pattern = '^(?:\$\s*)?usertests$' },
                    @{ Name = 'cmd_done_echo'; Pattern = '^(?:\$\s*)?echo USERTESTS_DONE$' },
                    @{ Name = 'done_seen'; Pattern = '^(?:\$\s*)?USERTESTS_DONE$' },
                    @{ Name = 'prompt_return'; Pattern = '^\$\s?' }
                )
            }
        }
        default {
            throw "Unsupported matrix scenario: $Scenario"
        }
    }
}

if ($Mode -eq 'single') {
    $singleResult = Invoke-SmokeCase `
        -CaseName 'single' `
        -CaseUartScript $UartScript `
        -RequiredPatterns @(
        '(?m)init: starting sh',
        '(?m)^(?:\$\s*)?echo HI$',
        '(?m)^HI$',
        '(?m)\$ ',
        '(?m)UART script injected:'
    ) `
        -OrderedMarkers @(
        'echo HI',
        'HI',
        '$ '
    ) `
        -SessionSteps @(
        @{ Name = 'shell_start'; Pattern = '^init: starting sh$' },
        @{ Name = 'prompt_ready'; Pattern = '^\$\s?' },
        @{ Name = 'cmd_echo'; Pattern = '^(?:\$\s*)?echo HI$' },
        @{ Name = 'cmd_output'; Pattern = '^HI$' },
        @{ Name = 'prompt_return'; Pattern = '^\$\s?' }
    )

    if (-not $singleResult.Passed) {
        Write-Host "[shell-smoke][single] Missing expected patterns:" -ForegroundColor Red
        $singleResult.MissingPatterns | ForEach-Object { Write-Host "  - $_" -ForegroundColor Red }
        throw '[shell-smoke] FAILED'
    }

    Write-Host '[shell-smoke] PASS: single scenario verified.' -ForegroundColor Green
    exit 0
}

$results = New-Object System.Collections.Generic.List[object]
foreach ($scenario in $MatrixScenarios) {
    $case = Get-MatrixCase -Scenario $scenario
    $result = Invoke-SmokeCase `
        -CaseName $case.Name `
        -CaseUartScript $case.UartScript `
        -RequiredPatterns $case.RequiredPatterns `
        -OrderedMarkers $case.OrderedMarkers `
        -SessionSteps $case.SessionSteps
    $results.Add($result)
}

$failed = @($results | Where-Object { -not $_.Passed })

Write-Host ''
Write-Host '[shell-smoke] Matrix summary:' -ForegroundColor Cyan
foreach ($result in $results) {
    if ($result.Passed) {
        Write-Host "  [PASS] $($result.Name)" -ForegroundColor Green
    }
    else {
        Write-Host "  [FAIL] $($result.Name)" -ForegroundColor Red
        if (-not [string]::IsNullOrWhiteSpace($result.FailureCategory)) {
            Write-Host "        category: $($result.FailureCategory)" -ForegroundColor DarkRed
        }
        $result.MissingPatterns | ForEach-Object {
            Write-Host "        missing: $_" -ForegroundColor Red
        }
        Write-Host "        log: $($result.LogFile)" -ForegroundColor DarkYellow
    }
}

$failedByCategory = @{
    startup   = @($failed | Where-Object { $_.FailureCategory -eq 'startup' }).Count
    injection = @($failed | Where-Object { $_.FailureCategory -eq 'injection' }).Count
    assertion = @($failed | Where-Object { $_.FailureCategory -eq 'assertion' }).Count
}

if ($failed.Count -gt 0) {
    Write-Host "[shell-smoke] Failure categories: startup=$($failedByCategory.startup), injection=$($failedByCategory.injection), assertion=$($failedByCategory.assertion)" -ForegroundColor DarkRed
}

if ($failed.Count -gt 0) {
    throw "[shell-smoke] Matrix FAILED: $($failed.Count)/$($results.Count) scenarios failed"
}

Write-Host "[shell-smoke] PASS: all $($results.Count) matrix scenarios verified." -ForegroundColor Green
