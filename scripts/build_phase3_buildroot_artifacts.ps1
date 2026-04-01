# myCPU Phase 3 Buildroot artifact builder (Windows PowerShell)
#
# Purpose:
#   Build qemu_riscv32_virt Buildroot images and copy key artifacts to artifacts/phase3.
#   Produces (expected):
#     - fw_jump.elf
#     - Image
#     - rootfs.ext2

param(
    [string]$BuildrootRepo = "https://github.com/buildroot/buildroot.git",
    [string]$BuildrootRef = "2024.02.1",
    [string]$WorkDir = "third_party\buildroot-riscv32-virt",
    [string]$ArtifactsDir = "artifacts\phase3",
    [string]$WslOutputDir = "",
    [int]$Jobs = 0,
    [switch]$SkipClone,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
Set-Location $repoRoot

function Ensure-Dir {
    param([string]$PathValue)
    if (-not (Test-Path $PathValue)) {
        New-Item -ItemType Directory -Path $PathValue -Force | Out-Null
    }
}

function Test-CommandAvailable {
    param([string]$Name)
    $cmd = Get-Command $Name -ErrorAction SilentlyContinue
    return ($null -ne $cmd)
}

function Require-Command {
    param([string]$Name)
    if (-not (Test-CommandAvailable -Name $Name)) {
        throw "[phase3-buildroot] required command not found: $Name"
    }
}

function Invoke-WslBash {
    param([string]$Script)
    $normalizedScript = $Script -replace "`r", ""
    $scriptBody = @(
        'set -euo pipefail'
        "export PATH='/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin'"
        $normalizedScript
    ) -join "`n"

    $encoded = [Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($scriptBody))
    $launcher = "printf '%s' '$encoded' | base64 -d | bash"
    & wsl -e bash -lc $launcher

    if ($LASTEXITCODE -ne 0) {
        throw "[phase3-buildroot] wsl command failed (exit=$LASTEXITCODE)"
    }
}

function Invoke-WslBashCapture {
    param([string]$Script)

    $normalizedScript = $Script -replace "`r", ""
    $scriptBody = @(
        'set -euo pipefail'
        "export PATH='/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin'"
        $normalizedScript
    ) -join "`n"

    $encoded = [Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($scriptBody))
    $launcher = "printf '%s' '$encoded' | base64 -d | bash"
    $result = (& wsl -e bash -lc $launcher | Out-String).Trim()

    if ($LASTEXITCODE -ne 0) {
        throw "[phase3-buildroot] wsl command failed (exit=$LASTEXITCODE)"
    }

    return $result
}

function Convert-ToWslPath {
    param([string]$WindowsPath)
    $wslPath = (& wsl -e wslpath -a $WindowsPath | Out-String).Trim()
    if ([string]::IsNullOrWhiteSpace($wslPath)) {
        throw "[phase3-buildroot] failed to convert path to WSL path: $WindowsPath"
    }
    return $wslPath
}

function Convert-WslToWindowsPath {
    param([string]$WslPath)
    $winPath = (& wsl -e wslpath -w $WslPath | Out-String).Trim()
    if ([string]::IsNullOrWhiteSpace($winPath)) {
        throw "[phase3-buildroot] failed to convert WSL path to Windows path: $WslPath"
    }
    return $winPath
}

function Assert-LastExit {
    param([string]$Action)
    if ($LASTEXITCODE -ne 0) {
        throw "[phase3-buildroot] $Action failed (exit=$LASTEXITCODE)"
    }
}

function Restore-WslGitSymlinks {
    param([string]$RepoWslPath)

    $script = @'
set -euo pipefail; cd "__REPO_WSL__"; git ls-files -s | awk '$1=="120000"{print $4}' | while read -r p; do target="$(git cat-file -p ":$p" | tr -d '\r\n')"; if [ ! -L "$p" ]; then rm -f "$p"; ln -s "$target" "$p"; fi; done
'@

    $script = $script.Replace('__REPO_WSL__', $RepoWslPath)
    Invoke-WslBash -Script $script
}

if (-not $SkipClone.IsPresent) {
    Require-Command git
}

$useWslForBuild = $false
if (-not $SkipBuild.IsPresent) {
    if (Test-CommandAvailable -Name make) {
        $useWslForBuild = $false
    }
    elseif (Test-CommandAvailable -Name wsl) {
        $useWslForBuild = $true
        Write-Host '[phase3-buildroot] local make not found, fallback to WSL make.' -ForegroundColor Yellow
        Invoke-WslBash -Script 'command -v make >/dev/null'
    }
    else {
        throw '[phase3-buildroot] required command not found: make (and wsl is unavailable)'
    }
}

$workAbs = Join-Path $repoRoot $WorkDir
$artifactsAbs = Join-Path $repoRoot $ArtifactsDir
Ensure-Dir -PathValue $artifactsAbs

if (-not $SkipClone.IsPresent) {
    if (-not (Test-Path $workAbs)) {
        $maxCloneAttempts = 2
        for ($attempt = 1; $attempt -le $maxCloneAttempts; $attempt++) {
            Write-Host "[phase3-buildroot] cloning Buildroot (attempt $attempt/$maxCloneAttempts): $BuildrootRepo" -ForegroundColor Cyan
            git clone $BuildrootRepo $workAbs
            if ($LASTEXITCODE -eq 0) {
                break
            }

            if ($attempt -eq $maxCloneAttempts) {
                Assert-LastExit -Action 'git clone'
            }

            Write-Host '[phase3-buildroot] clone failed, retrying ...' -ForegroundColor Yellow
            Start-Sleep -Seconds 2
        }
    }

    Push-Location $workAbs
    try {
        Write-Host "[phase3-buildroot] checking out ref: $BuildrootRef" -ForegroundColor Cyan
        git checkout $BuildrootRef
        if ($LASTEXITCODE -ne 0) {
            Write-Host '[phase3-buildroot] local checkout failed, fetching tags/remotes then retry ...' -ForegroundColor Yellow
            git fetch --tags --all --prune
            Assert-LastExit -Action 'git fetch'
            git checkout $BuildrootRef
            Assert-LastExit -Action 'git checkout'
        }
    }
    finally {
        Pop-Location
    }
}

if (-not (Test-Path $workAbs)) {
    throw "[phase3-buildroot] buildroot work dir not found: $workAbs"
}

if (-not $SkipBuild.IsPresent) {
    $jobCount = $Jobs
    if ($jobCount -le 0) {
        $jobCount = [Environment]::ProcessorCount
    }

    $imagesDir = $null

    if ($useWslForBuild) {
        $workWsl = Convert-ToWslPath -WindowsPath $workAbs
        $wslOutputResolved = $WslOutputDir
        if ([string]::IsNullOrWhiteSpace($wslOutputResolved)) {
            $wslOutputResolved = '${HOME}/.cache/mycpu-buildroot-riscv32-virt-output'
        }

        Write-Host '[phase3-buildroot] restoring git symlinks for WSL build ...' -ForegroundColor Cyan
        Restore-WslGitSymlinks -RepoWslPath $workWsl

        $lineEndingProbe = (& wsl -e bash -lc "export PATH='/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin'; set -euo pipefail; cd '$workWsl'; file support/scripts/setlocalversion" | Out-String).Trim()
        if ($lineEndingProbe -match 'CRLF') {
            Write-Host '[phase3-buildroot] detected CRLF checkout; normalizing to LF in WSL ...' -ForegroundColor Yellow
            Invoke-WslBash -Script "set -euo pipefail; cd '$workWsl'; git config core.autocrlf false; git reset --hard HEAD"
        }

        Write-Host "[phase3-buildroot] using WSL output dir: $wslOutputResolved" -ForegroundColor Cyan

        $defconfigScript = @'
cd '__WORK_WSL__'
output_dir="__OUTPUT_WSL__"
mkdir -p "$output_dir"
make O="$output_dir" qemu_riscv32_virt_defconfig
'@
        $defconfigScript = $defconfigScript.Replace('__WORK_WSL__', $workWsl).Replace('__OUTPUT_WSL__', $wslOutputResolved)

        Write-Host '[phase3-buildroot] running qemu_riscv32_virt_defconfig (WSL ext4 output) ...' -ForegroundColor Cyan
        Invoke-WslBash -Script $defconfigScript

        $buildScript = @'
cd '__WORK_WSL__'
output_dir="__OUTPUT_WSL__"
make O="$output_dir" -j __JOBS__
'@
        $buildScript = $buildScript.Replace('__WORK_WSL__', $workWsl).Replace('__OUTPUT_WSL__', $wslOutputResolved).Replace('__JOBS__', [string]$jobCount)

        Write-Host "[phase3-buildroot] building in WSL ext4 output (jobs=$jobCount) ..." -ForegroundColor Cyan
        Invoke-WslBash -Script $buildScript

        $resolvedOutputScript = @'
output_dir="__OUTPUT_WSL__"
eval echo "$output_dir"
'@
        $resolvedOutputScript = $resolvedOutputScript.Replace('__OUTPUT_WSL__', $wslOutputResolved)
        $resolvedOutputWsl = Invoke-WslBashCapture -Script $resolvedOutputScript

        $imagesResolveScript = @'
realpath "__IMAGES_WSL__"
'@
        $imagesResolveScript = $imagesResolveScript.Replace('__IMAGES_WSL__', "$resolvedOutputWsl/images")
        $imagesWsl = Invoke-WslBashCapture -Script $imagesResolveScript
        if ([string]::IsNullOrWhiteSpace($imagesWsl)) {
            throw '[phase3-buildroot] failed to resolve WSL images directory'
        }
        $imagesDir = Convert-WslToWindowsPath -WslPath $imagesWsl
    }
    else {
        Push-Location $workAbs
        try {
            Write-Host '[phase3-buildroot] running qemu_riscv32_virt_defconfig ...' -ForegroundColor Cyan
            make qemu_riscv32_virt_defconfig
            Assert-LastExit -Action 'make qemu_riscv32_virt_defconfig'

            Write-Host "[phase3-buildroot] building (jobs=$jobCount) ..." -ForegroundColor Cyan
            make -j $jobCount
            Assert-LastExit -Action 'make -j'
        }
        finally {
            Pop-Location
        }

        $imagesDir = Join-Path $workAbs 'output\images'
    }
}

if ($null -eq $imagesDir) {
    $imagesDir = Join-Path $workAbs 'output\images'
}
if (-not (Test-Path $imagesDir)) {
    throw "[phase3-buildroot] output images dir missing: $imagesDir"
}

$required = @(
    @{ Name = 'fw_jump.elf'; Source = (Join-Path $imagesDir 'fw_jump.elf'); Dest = (Join-Path $artifactsAbs 'fw_jump.elf') },
    @{ Name = 'Image'; Source = (Join-Path $imagesDir 'Image'); Dest = (Join-Path $artifactsAbs 'Image') },
    @{ Name = 'rootfs.ext2'; Source = (Join-Path $imagesDir 'rootfs.ext2'); Dest = (Join-Path $artifactsAbs 'rootfs.ext2') }
)

foreach ($item in $required) {
    if (-not (Test-Path $item.Source)) {
        throw "[phase3-buildroot] missing artifact from buildroot output: $($item.Name) ($($item.Source))"
    }
    Copy-Item -Path $item.Source -Destination $item.Dest -Force
    Write-Host "[phase3-buildroot] copied $($item.Name) -> $($item.Dest)" -ForegroundColor Green
}

Write-Host "[phase3-buildroot] DONE. artifacts ready in $artifactsAbs" -ForegroundColor Green
Write-Host "[phase3-buildroot] Next: powershell -ExecutionPolicy Bypass -File .\scripts\run_linux_phase3_acceptance.ps1 -AutoResolveArtifacts -AutoDtb -StrictUserlandMarker" -ForegroundColor Green
