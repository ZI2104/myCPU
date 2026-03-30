#!/bin/bash
# Build script for RISC-V test programs
# Requires: riscv32-unknown-elf-as, riscv32-unknown-elf-ld, riscv32-unknown-elf-objcopy

set -e

PROGRAMS_DIR="$(dirname "$0")"

# Check for RISC-V toolchain
if ! command -v riscv32-unknown-elf-as &> /dev/null; then
    echo "Error: riscv32-unknown-elf-as not found"
    echo "Please install the RISC-V toolchain"
    exit 1
fi

# Build hello program
echo "Building hello.elf..."
riscv32-unknown-elf-as -march=rv32i -mabi=ilp32 \
    "$PROGRAMS_DIR/hello.s" -o "$PROGRAMS_DIR/hello.o"

riscv32-unknown-elf-ld -T "$PROGRAMS_DIR/link.ld" \
    "$PROGRAMS_DIR/hello.o" -o "$PROGRAMS_DIR/hello.elf"

riscv32-unknown-elf-objcopy -O binary \
    "$PROGRAMS_DIR/hello.elf" "$PROGRAMS_DIR/hello.bin"

echo "Built: hello.elf, hello.bin"
echo "Done!"
