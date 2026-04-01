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
    [string]$SbiPath,

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
    [switch]$StrictUserlandMarker,

    [string]$ArtifactsDir = "artifacts\phase3",
    [switch]$AutoResolveArtifacts,
    [switch]$DownloadOpenSbiIfMissing,
    [switch]$SkipBuild,
    [switch]$AllowElfPayloadRaw,
    [string]$OpenSbiVersion = "1.8.1",
    [string]$OpenSbiDownloadUrl = "https://github.com/riscv-software-src/opensbi/releases/download/v1.8.1/opensbi-1.8.1-rv-bin.tar.xz"
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

function Resolve-ArtifactByPatterns {
    param(
        [string]$BaseDir,
        [string[]]$Patterns,
        [string]$Label
    )

    $resolvedBase = Resolve-Path $BaseDir -ErrorAction SilentlyContinue
    if ($null -eq $resolvedBase) {
        return $null
    }

    foreach ($pattern in $Patterns) {
        $hit = Get-ChildItem -Path $resolvedBase.Path -Recurse -File -Filter $pattern -ErrorAction SilentlyContinue |
            Sort-Object FullName |
            Select-Object -First 1
        if ($null -ne $hit) {
            Write-Host "[phase3-linux] Auto-resolved ${Label}: $($hit.FullName)" -ForegroundColor DarkCyan
            return $hit.FullName
        }
    }

    return $null
}

function Ensure-OpenSbiBinary {
    param(
        [string]$DestDir,
        [string]$Version,
        [string]$DownloadUrl
    )

    $resolvedDest = Resolve-Path $DestDir -ErrorAction SilentlyContinue
    if ($null -eq $resolvedDest) {
        New-Item -ItemType Directory -Path $DestDir -Force | Out-Null
        $resolvedDest = Resolve-Path $DestDir
    }

    $sbiExisting = Resolve-ArtifactByPatterns -BaseDir $resolvedDest.Path -Patterns @('fw_jump*.elf', 'fw_dynamic*.elf') -Label 'SBI image'
    if ($null -ne $sbiExisting) {
        return $sbiExisting
    }

    $archiveName = "opensbi-$Version-rv-bin.tar.xz"
    $archivePath = Join-Path $resolvedDest.Path $archiveName
    Write-Host "[phase3-linux] Downloading OpenSBI $Version from: $DownloadUrl" -ForegroundColor Cyan
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $archivePath -UseBasicParsing

    Write-Host "[phase3-linux] Extracting OpenSBI archive..." -ForegroundColor Cyan
    tar -xf $archivePath -C $resolvedDest.Path

    $candidate = Get-ChildItem -Path $resolvedDest.Path -Recurse -File -Filter 'fw_jump*.elf' -ErrorAction SilentlyContinue |
        Sort-Object `
            @{ Expression = {
                    if ($_.FullName -match 'ilp32\\generic') { return 0 }
                    if ($_.FullName -match 'ilp32\\qemu\\virt|qemu\\virt|generic') { return 1 }
                    if ($_.FullName -match 'lp64\\generic') { return 2 }
                    return 10
                }
            },
            @{ Expression = { $_.FullName } } |
        Select-Object -First 1

    if ($null -eq $candidate) {
        throw "[phase3-linux] OpenSBI archive extracted but fw_jump.elf not found under $($resolvedDest.Path)"
    }

    Write-Host "[phase3-linux] OpenSBI ready: $($candidate.FullName)" -ForegroundColor Green
    return $candidate.FullName
}

$artifactsAbs = Resolve-Path $ArtifactsDir -ErrorAction SilentlyContinue
if ($null -eq $artifactsAbs) {
    New-Item -ItemType Directory -Path $ArtifactsDir -Force | Out-Null
    $artifactsAbs = Resolve-Path $ArtifactsDir
}

if ($DownloadOpenSbiIfMissing.IsPresent -and [string]::IsNullOrWhiteSpace($SbiPath)) {
    $SbiPath = Ensure-OpenSbiBinary -DestDir $artifactsAbs.Path -Version $OpenSbiVersion -DownloadUrl $OpenSbiDownloadUrl
}

if ($AutoResolveArtifacts.IsPresent) {
    if ([string]::IsNullOrWhiteSpace($SbiPath)) {
        $SbiPath = Resolve-ArtifactByPatterns -BaseDir $artifactsAbs.Path -Patterns @('fw_jump*.elf', 'fw_dynamic*.elf') -Label 'SBI image'
    }

    if ([string]::IsNullOrWhiteSpace($PayloadPath)) {
        $PayloadPath = Resolve-ArtifactByPatterns -BaseDir $artifactsAbs.Path -Patterns @('Image', 'vmlinux', 'zImage', 'bzImage') -Label 'payload image'
    }

    if ([string]::IsNullOrWhiteSpace($VirtioDisk)) {
        $VirtioDisk = Resolve-ArtifactByPatterns -BaseDir $artifactsAbs.Path -Patterns @('rootfs.ext4', 'rootfs.ext2', 'rootfs*.img', '*.ext4', '*.ext2') -Label 'VirtIO disk image'
    }

    if ([string]::IsNullOrWhiteSpace($DtbPath) -and -not $AutoDtb.IsPresent) {
        $DtbPath = Resolve-ArtifactByPatterns -BaseDir $artifactsAbs.Path -Patterns @('virt*.dtb', 'qemu*.dtb', '*.dtb') -Label 'DTB image'
    }
}

if ([string]::IsNullOrWhiteSpace($SbiPath)) {
    throw "[phase3-linux] missing SBI image. Provide -SbiPath, or use -AutoResolveArtifacts, or use -DownloadOpenSbiIfMissing."
}

if ([string]::IsNullOrWhiteSpace($PayloadPath)) {
    throw "[phase3-linux] missing payload image. Provide -PayloadPath (Linux Image) or place Image/vmlinux under $($artifactsAbs.Path) and use -AutoResolveArtifacts."
}

$sbiAbs = Resolve-ExistingPath -PathValue $SbiPath -Label 'SBI image'
$payloadAbs = Resolve-ExistingPath -PathValue $PayloadPath -Label 'Payload image'

[byte[]]$payloadMagic = Get-Content -Path $payloadAbs -Encoding Byte -TotalCount 4
$isElfPayload = $payloadMagic.Count -ge 4 -and $payloadMagic[0] -eq 0x7F -and $payloadMagic[1] -eq 0x45 -and $payloadMagic[2] -eq 0x4C -and $payloadMagic[3] -eq 0x46
if ($isElfPayload -and -not $AllowElfPayloadRaw.IsPresent) {
    throw "[phase3-linux] payload appears to be ELF: $payloadAbs . This script expects a raw Linux Image for --linux-payload-addr. Provide Linux Image (non-ELF), or pass -AllowElfPayloadRaw only for debugging."
}

$diskAbs = $null
if (-not [string]::IsNullOrWhiteSpace($VirtioDisk)) {
    $diskAbs = Resolve-ExistingPath -PathValue $VirtioDisk -Label 'VirtIO disk image'
}

$dtbAbs = $null
if (-not [string]::IsNullOrWhiteSpace($DtbPath)) {
    $dtbAbs = Resolve-ExistingPath -PathValue $DtbPath -Label 'DTB image'
}

if (-not $SkipBuild.IsPresent) {
    Write-Host '[phase3-linux] Building release binary...' -ForegroundColor Cyan
    cargo build --release | Out-Null
}

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
