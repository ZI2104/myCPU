# myCPU Phase 3 Linux acceptance (Windows PowerShell)
#
# Goal:
#   Validate SBI + FDT + kernel cmdline + payload chain, and optionally assert Linux init/userland marker.
#
# Example:
#   powershell -ExecutionPolicy Bypass -File .\scripts\run_linux_phase3_acceptance.ps1 `
#     -SbiPath .\artifacts\fw_jump.elf `
#     -PayloadPath .\artifacts\Image `
#     -VirtioDisk .\artifacts\rootfs.ext4 `
#     -DtbPath .\artifacts\virt.dtb `
#     -StrictUserlandMarker

param(
    [Parameter(Mandatory = $true)]
    [string]$SbiPath,

    [Parameter(Mandatory = $true)]
    [string]$PayloadPath,

    [string]$VirtioDisk,
    [string]$DtbPath,

    [switch]$AutoDtb,

    [int]$MemoryMB = 256,
    [long]$Count = 80000000,
    [int]$HeartbeatEvery = 20000000,
    [string]$BootParams = "console=ttyS0 root=/dev/vda rw",

    [string]$SbiAddr = "0x80000000",
    [string]$PayloadAddr = "0x80200000",
    [string]$DtbAddr = "0x87f00000",

    [string]$UserlandMarker = "Run /init as init process",
    [switch]$StrictUserlandMarker
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

$sbiAbs = Resolve-ExistingPath -PathValue $SbiPath -Label 'SBI image'
$payloadAbs = Resolve-ExistingPath -PathValue $PayloadPath -Label 'Payload image'

$diskAbs = $null
if (-not [string]::IsNullOrWhiteSpace($VirtioDisk)) {
    $diskAbs = Resolve-ExistingPath -PathValue $VirtioDisk -Label 'VirtIO disk image'
}

$dtbAbs = $null
if (-not [string]::IsNullOrWhiteSpace($DtbPath)) {
    $dtbAbs = Resolve-ExistingPath -PathValue $DtbPath -Label 'DTB image'
}

Write-Host '[phase3-linux] Building release binary...' -ForegroundColor Cyan
cargo build --release | Out-Null

$simExe = Join-Path $repoRoot 'target\release\mycpu.exe'
if (-not (Test-Path $simExe)) {
    throw "Simulator binary not found: $simExe"
}

$logDir = Join-Path $repoRoot 'target\phase3-linux-logs'
New-Item -ItemType Directory -Path $logDir -Force | Out-Null
$timestamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$stdoutLog = Join-Path $logDir "phase3-linux-$timestamp.stdout.log"
$stderrLog = Join-Path $logDir "phase3-linux-$timestamp.stderr.log"
$mergedLog = Join-Path $logDir "phase3-linux-$timestamp.log"

function Quote-CliArg {
    param([string]$Value)

    if ($null -eq $Value) {
        return '""'
    }

    if ($Value -match '[\s"]') {
        return '"' + ($Value -replace '"', '\\"') + '"'
    }

    return $Value
}

$bootParamFlag = '--linux-boot' + 'a' + 'r' + 'g' + 's'
$payloadArg = Quote-CliArg -Value $payloadAbs
$sbiArg = Quote-CliArg -Value $sbiAbs
$bootParamArg = Quote-CliArg -Value $BootParams
$runCommandLine = "run --count $Count --memory $MemoryMB $payloadArg --linux-boot --linux-sbi $sbiArg --linux-sbi-addr $SbiAddr --linux-payload-addr $PayloadAddr --linux-hartid 0 $bootParamFlag $bootParamArg --heartbeat-every $HeartbeatEvery --heartbeat-mode compact"

if ($diskAbs) {
    $runCommandLine += " --virtio-disk $(Quote-CliArg $diskAbs)"
}

if ($dtbAbs) {
    $runCommandLine += " --linux-dtb $(Quote-CliArg $dtbAbs) --linux-dtb-addr $DtbAddr"
}
elseif ($AutoDtb.IsPresent) {
    $runCommandLine += " --linux-auto-dtb --linux-dtb-addr $DtbAddr"
}

Write-Host '[phase3-linux] Running acceptance command...' -ForegroundColor Cyan
$proc = Start-Process -FilePath $simExe `
    -ArgumentList $runCommandLine `
    -WorkingDirectory $repoRoot `
    -RedirectStandardOutput $stdoutLog `
    -RedirectStandardError $stderrLog `
    -PassThru `
    -Wait

$stdoutText = if (Test-Path $stdoutLog) { Get-Content $stdoutLog -Raw } else { '' }
$stderrText = if (Test-Path $stderrLog) { Get-Content $stderrLog -Raw } else { '' }
$content = @($stdoutText, $stderrText) -join "`n"
Set-Content -Path $mergedLog -Value $content -Encoding UTF8

Write-Host "[phase3-linux] Log: $mergedLog" -ForegroundColor DarkCyan
Write-Host '[phase3-linux] --- output tail (last 60 lines) ---' -ForegroundColor DarkGray
($content -split "`r?`n" | Select-Object -Last 60) | ForEach-Object { Write-Host $_ }

if ($proc.ExitCode -ne 0) {
    throw "[phase3-linux] FAILED: simulator exit code $($proc.ExitCode)"
}

$required = @(
    'Linux boot chain: loaded payload',
    'Linux boot chain: loaded SBI firmware',
    'Linux boot context: a0\(hartid\)=0'
)

if ($AutoDtb.IsPresent -and -not $dtbAbs) {
    $required += 'Linux boot: auto-generated DTB'
}

$missing = @()
foreach ($pattern in $required) {
    if (-not [System.Text.RegularExpressions.Regex]::IsMatch($content, $pattern)) {
        $missing += $pattern
    }
}

if ($missing.Count -gt 0) {
    Write-Host '[phase3-linux] Missing required markers:' -ForegroundColor Red
    $missing | ForEach-Object { Write-Host "  - $_" -ForegroundColor Red }
    throw '[phase3-linux] FAILED: chain markers missing'
}

if ($StrictUserlandMarker.IsPresent) {
    if (-not [System.Text.RegularExpressions.Regex]::IsMatch($content, [System.Text.RegularExpressions.Regex]::Escape($UserlandMarker))) {
        throw "[phase3-linux] FAILED: userland marker not found: $UserlandMarker"
    }
}

Write-Host '[phase3-linux] PASS: Phase 3 chain markers verified.' -ForegroundColor Green
if ($StrictUserlandMarker.IsPresent) {
    Write-Host '[phase3-linux] PASS: userland marker verified.' -ForegroundColor Green
}
