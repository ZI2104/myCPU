# myCPU Mario-style CPU validation (NEMU-inspired guest workload acceptance)
#
# Goal:
#   Run a guest program with deterministic input injection and assert CPU-behavior milestones.
#
# Example:
#   powershell -ExecutionPolicy Bypass -File .\scripts\run_mario_cpu_validation.ps1 `
#     -GuestBinary .\third_party\xv6-rv32\kernel\kernel `
#     -VirtioDisk .\third_party\xv6-rv32\fs.img `
#     -ExpectedMarkers "init: starting sh" `
#     -RequireInputFullyInjected

param(
    [Parameter(Mandatory = $true)]
    [string]$GuestBinary,

    [string]$VirtioDisk,

    [int]$MemoryMB = 128,
    [long]$Count = 120000000,
    [int]$HeartbeatEvery = 30000000,
    [ValidateSet('compact', 'diagnostic')]
    [string]$HeartbeatMode = 'compact',

    [string]$InputScript = 'right:down;right:up;btn_a:down;btn_a:up',
    [long]$InputInjectAt = 90000000,
    [int]$InputInjectEvery = 500000,

    [long]$MinInstructions = 1000000,
    [string[]]$ExpectedMarkers = @(),
    [switch]$RequireInputFullyInjected
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
Set-Location $repoRoot

function Resolve-ExistingPath {
    param([string]$PathValue, [string]$Label)
    $resolved = Resolve-Path $PathValue -ErrorAction SilentlyContinue
    if ($null -eq $resolved) {
        throw "$Label not found: $PathValue"
    }
    return $resolved.Path
}

$guestAbs = Resolve-ExistingPath -PathValue $GuestBinary -Label 'Guest binary'
$diskAbs = $null
if (-not [string]::IsNullOrWhiteSpace($VirtioDisk)) {
    $diskAbs = Resolve-ExistingPath -PathValue $VirtioDisk -Label 'VirtIO disk image'
}

Write-Host '[mario-cpu] Building release binary...' -ForegroundColor Cyan
$buildOut = Join-Path $env:TEMP 'mycpu-mario-cpu-build.stdout.log'
$buildErr = Join-Path $env:TEMP 'mycpu-mario-cpu-build.stderr.log'
$buildProc = Start-Process -FilePath 'cargo' `
    -ArgumentList @('build', '--release') `
    -WorkingDirectory $repoRoot `
    -RedirectStandardOutput $buildOut `
    -RedirectStandardError $buildErr `
    -PassThru `
    -Wait

if ($buildProc.ExitCode -ne 0) {
    $outText = if (Test-Path $buildOut) { Get-Content $buildOut -Raw } else { '' }
    $errText = if (Test-Path $buildErr) { Get-Content $buildErr -Raw } else { '' }
    throw "[mario-cpu] build failed with exit code $($buildProc.ExitCode)`n$outText`n$errText"
}

$simExe = Join-Path $repoRoot 'target\release\mycpu.exe'
if (-not (Test-Path $simExe)) {
    throw "[mario-cpu] simulator binary not found after build: $simExe"
}

$logDir = Join-Path $repoRoot 'target\mario-cpu-validation-logs'
New-Item -ItemType Directory -Path $logDir -Force | Out-Null
$timestamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$stdoutLog = Join-Path $logDir "mario-cpu-$timestamp.stdout.log"
$stderrLog = Join-Path $logDir "mario-cpu-$timestamp.stderr.log"
$mergedLog = Join-Path $logDir "mario-cpu-$timestamp.log"

$runParts = @(
    'run',
    '--count', $Count,
    '--heartbeat-every', $HeartbeatEvery,
    '--heartbeat-mode', $HeartbeatMode,
    '--memory', $MemoryMB,
    $guestAbs,
    '--input-script', $InputScript,
    '--input-inject-at', $InputInjectAt,
    '--input-inject-every', $InputInjectEvery
)

if ($diskAbs) {
    $runParts += @('--virtio-disk', $diskAbs)
}

Write-Host '[mario-cpu] Running guest validation...' -ForegroundColor Cyan
$runProc = Start-Process -FilePath $simExe `
    -ArgumentList $runParts `
    -WorkingDirectory $repoRoot `
    -RedirectStandardOutput $stdoutLog `
    -RedirectStandardError $stderrLog `
    -PassThru `
    -Wait

$stdoutText = if (Test-Path $stdoutLog) { Get-Content $stdoutLog -Raw } else { '' }
$stderrText = if (Test-Path $stderrLog) { Get-Content $stderrLog -Raw } else { '' }
$content = @($stdoutText, $stderrText) -join "`n"
Set-Content -Path $mergedLog -Value $content -Encoding UTF8

Write-Host "[mario-cpu] Log: $mergedLog" -ForegroundColor DarkCyan
Write-Host '[mario-cpu] --- output tail (last 60 lines) ---' -ForegroundColor DarkGray
($content -split "`r?`n" | Select-Object -Last 60) | ForEach-Object { Write-Host $_ }

if ($runProc.ExitCode -ne 0) {
    throw "[mario-cpu] FAILED: simulator exit code $($runProc.ExitCode)"
}

$instrMatch = [System.Text.RegularExpressions.Regex]::Match($content, 'Instructions executed:\s+(?<count>\d+)')
if (-not $instrMatch.Success) {
    throw '[mario-cpu] FAILED: missing instruction summary marker'
}

$executed = [long]$instrMatch.Groups['count'].Value
if ($executed -lt $MinInstructions) {
    throw "[mario-cpu] FAILED: executed instructions $executed is below threshold $MinInstructions"
}

$injectMatch = [System.Text.RegularExpressions.Regex]::Match($content, 'Input script injected:\s+(?<done>\d+)\/(?<total>\d+) actions')
if (-not $injectMatch.Success) {
    throw '[mario-cpu] FAILED: missing input injection summary marker'
}

$doneActions = [long]$injectMatch.Groups['done'].Value
$totalActions = [long]$injectMatch.Groups['total'].Value
if ($doneActions -le 0 -or $totalActions -le 0) {
    throw "[mario-cpu] FAILED: invalid input injection counters done=$doneActions total=$totalActions"
}

if ($RequireInputFullyInjected.IsPresent -and $doneActions -ne $totalActions) {
    throw "[mario-cpu] FAILED: input actions not fully injected ($doneActions/$totalActions)"
}

$missingMarkers = @()
foreach ($marker in $ExpectedMarkers) {
    if ($content.IndexOf($marker, [System.StringComparison]::Ordinal) -lt 0) {
        $missingMarkers += $marker
    }
}

if ($missingMarkers.Count -gt 0) {
    Write-Host '[mario-cpu] Missing expected markers:' -ForegroundColor Red
    $missingMarkers | ForEach-Object { Write-Host "  - $_" -ForegroundColor Red }
    throw '[mario-cpu] FAILED: expected markers missing'
}

Write-Host "[mario-cpu] PASS: executed=$executed input=$doneActions/$totalActions markers=$($ExpectedMarkers.Count)" -ForegroundColor Green
