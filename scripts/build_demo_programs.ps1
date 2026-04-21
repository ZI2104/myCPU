# Build demo RV32I programs used by DEMO_GUIDE.md
# Outputs: tests/programs/{hello,fib,test}.elf and .bin

param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
$programDir = Join-Path $repoRoot 'tests/programs'
$linkerScript = Join-Path $programDir 'link.ld'

if (-not (Test-Path $linkerScript)) {
    throw "Linker script not found: $linkerScript"
}

function Convert-ToWslPath {
    param(
        [Parameter(Mandatory = $true)]
        [string]$WindowsPath
    )

    if ($WindowsPath -match '^([A-Za-z]):\\(.*)$') {
        $drive = $matches[1].ToLower()
        $rest = $matches[2] -replace '\\', '/'
        return "/mnt/$drive/$rest"
    }

    throw "Cannot convert Windows path to WSL path: $WindowsPath"
}

function Test-WslToolchain {
    $wsl = Get-Command 'wsl' -ErrorAction SilentlyContinue
    if ($null -eq $wsl) {
        return $false
    }

    & wsl bash -lc "if (command -v riscv32-unknown-elf-gcc >/dev/null 2>&1 && command -v riscv32-unknown-elf-objcopy >/dev/null 2>&1) || (command -v riscv64-unknown-elf-gcc >/dev/null 2>&1 && command -v riscv64-unknown-elf-objcopy >/dev/null 2>&1); then exit 0; else exit 1; fi"
    return $LASTEXITCODE -eq 0
}

function Resolve-Toolchain {
    $riscv32Gcc = Get-Command 'riscv32-unknown-elf-gcc' -ErrorAction SilentlyContinue
    $riscv32Objcopy = Get-Command 'riscv32-unknown-elf-objcopy' -ErrorAction SilentlyContinue
    if ($null -ne $riscv32Gcc -and $null -ne $riscv32Objcopy) {
        return @{
            Mode    = 'Native'
            Gcc     = $riscv32Gcc.Source
            Objcopy = $riscv32Objcopy.Source
            Name    = 'riscv32-unknown-elf'
        }
    }

    $riscv64Gcc = Get-Command 'riscv64-unknown-elf-gcc' -ErrorAction SilentlyContinue
    $riscv64Objcopy = Get-Command 'riscv64-unknown-elf-objcopy' -ErrorAction SilentlyContinue
    if ($null -ne $riscv64Gcc -and $null -ne $riscv64Objcopy) {
        return @{
            Mode    = 'Native'
            Gcc     = $riscv64Gcc.Source
            Objcopy = $riscv64Objcopy.Source
            Name    = 'riscv64-unknown-elf (rv32 target mode)'
        }
    }

    if (Test-WslToolchain) {
        return @{
            Mode = 'Wsl'
            Name = 'WSL riscv-unknown-elf toolchain'
        }
    }

    throw @"
RISC-V cross toolchain not found.
Please install one of (or make sure WSL toolchain is available):
  - riscv32-unknown-elf-gcc + riscv32-unknown-elf-objcopy
  - riscv64-unknown-elf-gcc + riscv64-unknown-elf-objcopy
"@
}

function Build-DemoProgram {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [Parameter(Mandatory = $true)]
        [string]$Gcc,
        [Parameter(Mandatory = $true)]
        [string]$Objcopy
    )

    $src = Join-Path $programDir "$Name.s"
    $elf = Join-Path $programDir "$Name.elf"
    $bin = Join-Path $programDir "$Name.bin"

    if (-not (Test-Path $src)) {
        throw "Demo source file not found: $src"
    }

    Write-Host "[demo-build] Building $Name.elf ..." -ForegroundColor Cyan
    & $Gcc -march=rv32i -mabi=ilp32 -nostdlib -T $linkerScript $src -o $elf
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to build $Name.elf"
    }

    & $Objcopy -O binary $elf $bin
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to build $Name.bin"
    }

    Write-Host "[demo-build] Built: $elf" -ForegroundColor Green
    Write-Host "[demo-build] Built: $bin" -ForegroundColor Green
}

$toolchain = Resolve-Toolchain
Write-Host "[demo-build] Using toolchain: $($toolchain.Name)" -ForegroundColor Yellow

if ($toolchain.Mode -eq 'Wsl') {
    $wslRepoRoot = Convert-ToWslPath -WindowsPath $repoRoot.Path
    & wsl bash -lc "cd '$wslRepoRoot' && bash tests/programs/build.sh"
    if ($LASTEXITCODE -ne 0) {
        throw "WSL build failed with exit code $LASTEXITCODE"
    }
    Write-Host '[demo-build] Done (via WSL).' -ForegroundColor Green
    return
}

$programs = @('hello', 'fib', 'test')
foreach ($name in $programs) {
    Build-DemoProgram -Name $name -Gcc $toolchain.Gcc -Objcopy $toolchain.Objcopy
}

Write-Host '[demo-build] Done.' -ForegroundColor Green
