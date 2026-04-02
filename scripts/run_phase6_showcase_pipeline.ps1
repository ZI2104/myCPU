# myCPU Phase 6 showcase pipeline (Windows PowerShell)
#
# Goal:
#   Provide one-command orchestration for xv6 -> Linux -> game/input -> NPU/LPU checks.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File .\scripts\run_phase6_showcase_pipeline.ps1
#   powershell -ExecutionPolicy Bypass -File .\scripts\run_phase6_showcase_pipeline.ps1 -EnableLinux

param(
    [switch]$SkipBuild,
    [switch]$SkipXv6,
    [switch]$SkipPhase4,
    [switch]$SkipCoprocessor,
    [switch]$SkipFrontendBuild,

    # Linux stage is disabled by default because artifacts may be missing in many workspaces.
    [switch]$EnableLinux,
    [switch]$StrictLinuxUserland,

    [string]$LogDir = "target\phase6-demo-logs"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
Set-Location $repoRoot

$logAbs = Join-Path $repoRoot $LogDir
New-Item -ItemType Directory -Path $logAbs -Force | Out-Null

$timestamp = Get-Date -Format 'yyyyMMdd-HHmmss'

$results = New-Object System.Collections.Generic.List[object]

function Invoke-Stage {
    param(
        [string]$Name,
        [string]$FilePath,
        [string[]]$ArgumentList,
        [switch]$Required,
        [switch]$Skip
    )

    if ($Skip.IsPresent) {
        Write-Host "[phase6][$Name] SKIP" -ForegroundColor Yellow
        $results.Add([PSCustomObject]@{
                Name     = $Name
                Status   = 'SKIP'
                Required = $Required.IsPresent
                ExitCode = 0
                LogFile  = ''
            })
        return
    }

    $safeName = ($Name -replace '[^a-zA-Z0-9_-]', '_').ToLowerInvariant()
    $stdoutLog = Join-Path $logAbs "$timestamp-$safeName.stdout.log"
    $stderrLog = Join-Path $logAbs "$timestamp-$safeName.stderr.log"
    $mergedLog = Join-Path $logAbs "$timestamp-$safeName.log"

    Write-Host "[phase6][$Name] RUN" -ForegroundColor Cyan
    $proc = Start-Process -FilePath $FilePath `
        -ArgumentList $ArgumentList `
        -WorkingDirectory $repoRoot `
        -RedirectStandardOutput $stdoutLog `
        -RedirectStandardError $stderrLog `
        -PassThru `
        -Wait

    $stdoutText = if (Test-Path $stdoutLog) { Get-Content $stdoutLog -Raw } else { '' }
    $stderrText = if (Test-Path $stderrLog) { Get-Content $stderrLog -Raw } else { '' }
    Set-Content -Path $mergedLog -Value (@($stdoutText, $stderrText) -join "`n") -Encoding UTF8

    if ($proc.ExitCode -eq 0) {
        Write-Host "[phase6][$Name] PASS" -ForegroundColor Green
        $results.Add([PSCustomObject]@{
                Name     = $Name
                Status   = 'PASS'
                Required = $Required.IsPresent
                ExitCode = 0
                LogFile  = $mergedLog
            })
        return
    }

    Write-Host "[phase6][$Name] FAIL (exit=$($proc.ExitCode))" -ForegroundColor Red
    Write-Host "[phase6][$Name] log: $mergedLog" -ForegroundColor DarkYellow
    $results.Add([PSCustomObject]@{
            Name     = $Name
            Status   = 'FAIL'
            Required = $Required.IsPresent
            ExitCode = $proc.ExitCode
            LogFile  = $mergedLog
        })
}

if (-not $SkipBuild.IsPresent) {
    Invoke-Stage -Name 'build-release' -FilePath 'cargo' -ArgumentList @('build', '--release') -Required
}
else {
    Write-Host '[phase6][build-release] SKIP by flag -SkipBuild' -ForegroundColor Yellow
}

Invoke-Stage -Name 'xv6-shell-matrix' -FilePath 'powershell' -ArgumentList @(
    '-NoProfile',
    '-ExecutionPolicy', 'Bypass',
    '-File', '.\scripts\run_xv6_shell_smoke.ps1',
    '-Mode', 'matrix'
) -Required -Skip:$SkipXv6

Invoke-Stage -Name 'phase4-host-demo' -FilePath 'powershell' -ArgumentList @(
    '-NoProfile',
    '-ExecutionPolicy', 'Bypass',
    '-File', '.\scripts\run_phase4_input_framebuffer_acceptance.ps1',
    '-Mode', 'host-demo',
    '-SkipBuild'
) -Required -Skip:$SkipPhase4

Invoke-Stage -Name 'phase4-guest-demo' -FilePath 'powershell' -ArgumentList @(
    '-NoProfile',
    '-ExecutionPolicy', 'Bypass',
    '-File', '.\scripts\run_phase4_input_framebuffer_acceptance.ps1',
    '-Mode', 'guest-binary',
    '-SkipBuild'
) -Required -Skip:$SkipPhase4

if ($EnableLinux.IsPresent) {
    $linuxArgs = @(
        '-NoProfile',
        '-ExecutionPolicy', 'Bypass',
        '-File', '.\scripts\run_linux_phase3_acceptance.ps1',
        '-AutoResolveArtifacts',
        '-AutoDtb',
        '-SkipBuild'
    )
    if ($StrictLinuxUserland.IsPresent) {
        $linuxArgs += '-StrictUserlandMarker'
    }

    Invoke-Stage -Name 'linux-phase3-acceptance' -FilePath 'powershell' -ArgumentList $linuxArgs -Required
}
else {
    Write-Host '[phase6][linux-phase3-acceptance] SKIP (enable with -EnableLinux)' -ForegroundColor Yellow
    $results.Add([PSCustomObject]@{
            Name     = 'linux-phase3-acceptance'
            Status   = 'SKIP'
            Required = $false
            ExitCode = 0
            LogFile  = ''
        })
}

Invoke-Stage -Name 'coprocessor-fastpath-tests' -FilePath 'cargo' -ArgumentList @(
    'test',
    'custom0_',
    '--lib'
) -Required -Skip:$SkipCoprocessor

Invoke-Stage -Name 'coprocessor-dma-bridge-tests' -FilePath 'cargo' -ArgumentList @(
    'test',
    'descriptor_notify_bridge',
    '--lib'
) -Required -Skip:$SkipCoprocessor

Invoke-Stage -Name 'frontend-build' -FilePath 'powershell' -ArgumentList @(
    '-NoProfile',
    '-Command',
    'Set-Location ''./frontend''; npm run build'
) -Required -Skip:$SkipFrontendBuild

Write-Host ''
Write-Host '[phase6] Summary:' -ForegroundColor Cyan
foreach ($item in $results) {
    $color = switch ($item.Status) {
        'PASS' { 'Green' }
        'FAIL' { 'Red' }
        default { 'Yellow' }
    }

    $requiredTag = if ($item.Required) { 'required' } else { 'optional' }
    Write-Host ("  [{0}] {1} ({2})" -f $item.Status, $item.Name, $requiredTag) -ForegroundColor $color
    if ($item.Status -eq 'FAIL' -and -not [string]::IsNullOrWhiteSpace($item.LogFile)) {
        Write-Host "        log: $($item.LogFile)" -ForegroundColor DarkYellow
    }
}

$requiredFailed = @($results | Where-Object { $_.Required -and $_.Status -eq 'FAIL' })
if ($requiredFailed.Count -gt 0) {
    throw "[phase6] FAILED: $($requiredFailed.Count) required stage(s) failed."
}

Write-Host '[phase6] PASS: showcase pipeline completed (required stages).' -ForegroundColor Green