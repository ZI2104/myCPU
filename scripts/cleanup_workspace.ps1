# myCPU Workspace Cleanup Script (Windows PowerShell)
#
# Usage:
#   # 常规清理（默认）
#   powershell -ExecutionPolicy Bypass -File .\scripts\cleanup_workspace.ps1
#
#   # 深度瘦身：删除可重建构建缓存（target/buildroot output/frontend cache）
#   powershell -ExecutionPolicy Bypass -File .\scripts\cleanup_workspace.ps1 -PruneBuildCaches
#
#   # 可选：额外清理本地 third_party 源码缓存
#   powershell -ExecutionPolicy Bypass -File .\scripts\cleanup_workspace.ps1 -PurgeXv6

param(
    [switch]$PurgeXv6,
    [switch]$PruneBuildCaches,
    [switch]$PurgePhase3Artifacts
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
Set-Location $repoRoot

Write-Host '[cleanup] Repository root:' $repoRoot -ForegroundColor Cyan
$mode = if ($PruneBuildCaches) { 'deep' } else { 'regular' }
Write-Host ('[cleanup] Mode: {0}' -f $mode) -ForegroundColor Cyan

$removed = New-Object System.Collections.Generic.List[object]

function Get-PathSizeBytes {
    param([string]$Path)

    if (-not (Test-Path $Path)) {
        return [int64]0
    }

    $item = Get-Item $Path -Force
    if ($item.PSIsContainer) {
        $files = Get-ChildItem $Path -Recurse -File -ErrorAction SilentlyContinue
        if ($null -eq $files) {
            return [int64]0
        }

        $measure = $files | Measure-Object -Property Length -Sum
        if ($null -eq $measure -or $null -eq $measure.Sum) {
            return [int64]0
        }

        return [int64]$measure.Sum
    }

    return [int64]$item.Length
}

function Format-Bytes {
    param([int64]$Bytes)

    if ($Bytes -ge 1GB) { return ('{0:N2} GB' -f ($Bytes / 1GB)) }
    if ($Bytes -ge 1MB) { return ('{0:N2} MB' -f ($Bytes / 1MB)) }
    if ($Bytes -ge 1KB) { return ('{0:N2} KB' -f ($Bytes / 1KB)) }
    return ('{0} B' -f $Bytes)
}

function Remove-IfExists {
    param(
        [string]$Path,
        [string]$Reason
    )

    if (Test-Path $Path) {
        $sizeBytes = Get-PathSizeBytes -Path $Path
        Remove-Item $Path -Recurse -Force
        $removed.Add([PSCustomObject]@{
                Path   = $Path
                Reason = $Reason
                Size   = $sizeBytes
            })
    }
}

# 1) 常规清理：测试临时产物 + 各验收日志目录
Remove-IfExists -Path 'tests/programs/boot_loop.bin' -Reason 'transient smoke binary'
Remove-IfExists -Path 'tests/programs/mockfs.img' -Reason 'transient mock fs image'

Remove-IfExists -Path 'target/shell-smoke-logs' -Reason 'generated acceptance logs'
Remove-IfExists -Path 'target/phase3-linux-logs' -Reason 'generated acceptance logs'
Remove-IfExists -Path 'target/phase4-acceptance-logs' -Reason 'generated acceptance logs'
Remove-IfExists -Path 'target/mario-cpu-validation-logs' -Reason 'generated acceptance logs'
Remove-IfExists -Path 'target/phase6-demo-logs' -Reason 'generated acceptance logs'
Remove-IfExists -Path 'target/tmp' -Reason 'generated temporary files'

# 2) 深度瘦身：可重建缓存
if ($PruneBuildCaches) {
    Remove-IfExists -Path 'target' -Reason 'rust build cache (rebuildable)'
    Remove-IfExists -Path 'third_party/buildroot-riscv32-virt/output' -Reason 'buildroot generated output (rebuildable)'
    Remove-IfExists -Path 'frontend/node_modules' -Reason 'frontend dependency cache (reinstallable)'
    Remove-IfExists -Path 'frontend/dist' -Reason 'frontend build output (rebuildable)'
    Remove-IfExists -Path 'tmp' -Reason 'workspace temporary files'
}

# 3) 可选清理 third_party 本地源码缓存
if ($PurgeXv6) {
    Remove-IfExists -Path 'third_party/xv6-riscv' -Reason 'optional local xv6 source cache'
    Remove-IfExists -Path 'third_party/xv6-rv32' -Reason 'optional local xv6 source cache'
}

# 4) 可选清理 phase3 工件目录
if ($PurgePhase3Artifacts) {
    Remove-IfExists -Path 'artifacts/phase3' -Reason 'phase3 local artifacts cache'
}

if ($removed.Count -eq 0) {
    Write-Host '[cleanup] Nothing to remove. Workspace already clean for known transient/cache files.' -ForegroundColor Green
}
else {
    $totalBytes = [int64](($removed | Measure-Object -Property Size -Sum).Sum)
    Write-Host '[cleanup] Removed paths:' -ForegroundColor Yellow
    $removed | Sort-Object Size -Descending | ForEach-Object {
        Write-Host ('  - {0}  ({1})  [{2}]' -f $_.Path, (Format-Bytes -Bytes $_.Size), $_.Reason)
    }
    Write-Host ('[cleanup] Reclaimed: {0}' -f (Format-Bytes -Bytes $totalBytes)) -ForegroundColor Green
}

if (Test-Path 'tests/programs') {
    Write-Host '[cleanup] Remaining tests/programs files:' -ForegroundColor Cyan
    Get-ChildItem 'tests/programs' | Select-Object Name, Length | Format-Table -AutoSize
}
