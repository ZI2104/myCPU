# myCPU Phase 3 artifact setup helper (Windows PowerShell)
#
# Purpose:
#   Prepare artifact folder for run_linux_phase3_acceptance.ps1
#   - Download OpenSBI prebuilt binary package and resolve fw_jump.elf
#   - Optionally copy payload/rootfs/dtb from user-provided paths
#   - Optional smoke fallback from xv6-rv32 assets

param(
    [string]$ArtifactsDir = "artifacts\phase3",
    [string]$OpenSbiVersion = "1.8.1",
    [string]$OpenSbiDownloadUrl = "https://github.com/riscv-software-src/opensbi/releases/download/v1.8.1/opensbi-1.8.1-rv-bin.tar.xz",

    [string]$PayloadPath,
    [string]$VirtioDiskPath,
    [string]$DtbPath,

    [switch]$UseXv6SmokeFallback
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
Set-Location $repoRoot

function Resolve-ExistingPath {
    param([string]$PathValue, [string]$Label)
    $resolved = Resolve-Path $PathValue -ErrorAction SilentlyContinue
    if ($null -eq $resolved) {
        throw "[phase3-setup] $Label not found: $PathValue"
    }
    return $resolved.Path
}

function Resolve-ArtifactByPatterns {
    param(
        [string]$BaseDir,
        [string[]]$Patterns
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

    $existing = Resolve-ArtifactByPatterns -BaseDir $DestDir -Patterns @('fw_jump*.elf', 'fw_dynamic*.elf')
    if ($null -ne $existing) {
        Write-Host "[phase3-setup] Reuse OpenSBI: $existing" -ForegroundColor DarkCyan
        return $existing
    }

    New-Item -ItemType Directory -Path $DestDir -Force | Out-Null
    $archiveName = "opensbi-$Version-rv-bin.tar.xz"
    $archivePath = Join-Path $DestDir $archiveName

    Write-Host "[phase3-setup] Download OpenSBI $Version ..." -ForegroundColor Cyan
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $archivePath -UseBasicParsing

    Write-Host "[phase3-setup] Extract OpenSBI package..." -ForegroundColor Cyan
    tar -xf $archivePath -C $DestDir

    $candidate = Get-ChildItem -Path $DestDir -Recurse -File -Filter 'fw_jump*.elf' -ErrorAction SilentlyContinue |
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
        throw "[phase3-setup] OpenSBI extracted but fw_jump.elf not found in $DestDir"
    }

    Write-Host "[phase3-setup] OpenSBI ready: $($candidate.FullName)" -ForegroundColor Green
    return $candidate.FullName
}

$artifactsAbs = Resolve-Path $ArtifactsDir -ErrorAction SilentlyContinue
if ($null -eq $artifactsAbs) {
    New-Item -ItemType Directory -Path $ArtifactsDir -Force | Out-Null
    $artifactsAbs = Resolve-Path $ArtifactsDir
}

$openSbiPath = Ensure-OpenSbiBinary -DestDir $artifactsAbs.Path -Version $OpenSbiVersion -DownloadUrl $OpenSbiDownloadUrl

if (-not [string]::IsNullOrWhiteSpace($PayloadPath)) {
    $payloadAbs = Resolve-ExistingPath -PathValue $PayloadPath -Label 'Payload image'
    Copy-Item -Path $payloadAbs -Destination (Join-Path $artifactsAbs.Path 'Image') -Force
    Write-Host "[phase3-setup] Copied payload -> $($artifactsAbs.Path)\Image" -ForegroundColor DarkCyan
}

if (-not [string]::IsNullOrWhiteSpace($VirtioDiskPath)) {
    $diskAbs = Resolve-ExistingPath -PathValue $VirtioDiskPath -Label 'VirtIO disk image'
    Copy-Item -Path $diskAbs -Destination (Join-Path $artifactsAbs.Path 'rootfs.ext4') -Force
    Write-Host "[phase3-setup] Copied rootfs -> $($artifactsAbs.Path)\rootfs.ext4" -ForegroundColor DarkCyan
}

if (-not [string]::IsNullOrWhiteSpace($DtbPath)) {
    $dtbAbs = Resolve-ExistingPath -PathValue $DtbPath -Label 'DTB image'
    Copy-Item -Path $dtbAbs -Destination (Join-Path $artifactsAbs.Path 'virt.dtb') -Force
    Write-Host "[phase3-setup] Copied dtb -> $($artifactsAbs.Path)\virt.dtb" -ForegroundColor DarkCyan
}

if ($UseXv6SmokeFallback.IsPresent) {
    $xv6Kernel = Join-Path $repoRoot 'third_party\xv6-rv32\kernel\kernel'
    $xv6Fs = Join-Path $repoRoot 'third_party\xv6-rv32\fs.img'

    if (Test-Path $xv6Kernel) {
        Copy-Item -Path $xv6Kernel -Destination (Join-Path $artifactsAbs.Path 'xv6-kernel.elf') -Force
        Write-Host "[phase3-setup] xv6 fallback payload -> $($artifactsAbs.Path)\\xv6-kernel.elf" -ForegroundColor Yellow
    }

    if (Test-Path $xv6Fs) {
        Copy-Item -Path $xv6Fs -Destination (Join-Path $artifactsAbs.Path 'xv6-fs.img') -Force
        Write-Host "[phase3-setup] xv6 fallback disk -> $($artifactsAbs.Path)\\xv6-fs.img" -ForegroundColor Yellow
    }
}

$summary = @(
    "[phase3-setup] DONE",
    "  artifacts: $($artifactsAbs.Path)",
    "  sbi:       $openSbiPath",
    "  payload:   $(Resolve-ArtifactByPatterns -BaseDir $artifactsAbs.Path -Patterns @('Image','vmlinux','zImage','bzImage'))",
    "  rootfs:    $(Resolve-ArtifactByPatterns -BaseDir $artifactsAbs.Path -Patterns @('rootfs.ext4','rootfs.ext2','rootfs*.img','*.ext4','*.ext2'))",
    "  dtb:       $(Resolve-ArtifactByPatterns -BaseDir $artifactsAbs.Path -Patterns @('virt*.dtb','qemu*.dtb','*.dtb'))"
)
$summary | ForEach-Object { Write-Host $_ }

Write-Host "[phase3-setup] Next: run Phase 3 acceptance with -AutoResolveArtifacts (and -StrictUserlandMarker when Linux rootfs is ready)." -ForegroundColor Green
