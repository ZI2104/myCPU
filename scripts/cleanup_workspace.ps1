# myCPU Workspace Cleanup Script (Windows PowerShell)
# Usage:
#   powershell -ExecutionPolicy Bypass -File .\scripts\cleanup_workspace.ps1
#   powershell -ExecutionPolicy Bypass -File .\scripts\cleanup_workspace.ps1 -PurgeXv6

param(
    [switch]$PurgeXv6
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
Set-Location $repoRoot

Write-Host '[cleanup] Repository root:' $repoRoot -ForegroundColor Cyan

$removed = New-Object System.Collections.Generic.List[string]

function Remove-IfExists {
    param([string]$Path)

    if (Test-Path $Path) {
        Remove-Item $Path -Recurse -Force
        $removed.Add($Path)
    }
}

# Transient artifacts from startup/virtio smoke runs
Remove-IfExists 'tests/programs/boot_loop.bin'
Remove-IfExists 'tests/programs/mockfs.img'

# Optional: remove local xv6 clone under third_party cache
if ($PurgeXv6) {
    Remove-IfExists 'third_party/xv6-riscv'
}

if ($removed.Count -eq 0) {
    Write-Host '[cleanup] Nothing to remove. Workspace already clean for known transient files.' -ForegroundColor Green
}
else {
    Write-Host '[cleanup] Removed files/directories:' -ForegroundColor Yellow
    $removed | ForEach-Object { Write-Host ('  - ' + $_) }
}

Write-Host '[cleanup] Remaining tests/programs files:' -ForegroundColor Cyan
Get-ChildItem 'tests/programs' | Select-Object Name, Length | Format-Table -AutoSize
