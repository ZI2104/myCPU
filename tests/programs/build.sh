#!/bin/bash
# Build script for RISC-V test programs
# Supports either:
#   - riscv32-unknown-elf-gcc + riscv32-unknown-elf-objcopy
#   - riscv64-unknown-elf-gcc + riscv64-unknown-elf-objcopy (with RV32 flags)

set -e

PROGRAMS_DIR="$(dirname "$0")"

# Resolve toolchain prefix
if command -v riscv32-unknown-elf-gcc >/dev/null 2>&1 && command -v riscv32-unknown-elf-objcopy >/dev/null 2>&1; then
    TOOL_PREFIX="riscv32-unknown-elf"
elif command -v riscv64-unknown-elf-gcc >/dev/null 2>&1 && command -v riscv64-unknown-elf-objcopy >/dev/null 2>&1; then
    TOOL_PREFIX="riscv64-unknown-elf"
else
    echo "Error: no supported RISC-V cross toolchain found."
    echo "Need one of:"
    echo "  - riscv32-unknown-elf-gcc + riscv32-unknown-elf-objcopy"
    echo "  - riscv64-unknown-elf-gcc + riscv64-unknown-elf-objcopy"
    exit 1
fi

CC="${TOOL_PREFIX}-gcc"
OBJCOPY="${TOOL_PREFIX}-objcopy"

echo "Using toolchain: ${TOOL_PREFIX}"

build_asm_program() {
    local name="$1"

    echo "Building ${name}.elf..."
    "$CC" -march=rv32i -mabi=ilp32 -nostdlib -T "$PROGRAMS_DIR/link.ld" \
        "$PROGRAMS_DIR/${name}.s" -o "$PROGRAMS_DIR/${name}.elf"

    "$OBJCOPY" -O binary \
        "$PROGRAMS_DIR/${name}.elf" "$PROGRAMS_DIR/${name}.bin"

    echo "Built: ${name}.elf, ${name}.bin"
}

for demo in hello fib test; do
    build_asm_program "$demo"
done

echo "Built RV32I demo binaries: hello/fib/test"

# Build NPU vector example (prefer assembly if present)
echo "Building npu_vector_example.elf..."
if [ -f "$PROGRAMS_DIR/npu_vector_example.s" ]; then
    "$CC" -march=rv32i -mabi=ilp32 -nostdlib -T "$PROGRAMS_DIR/link.ld" \
        "$PROGRAMS_DIR/npu_vector_example.s" -o "$PROGRAMS_DIR/npu_vector_example.elf"
else
    "$CC" -march=rv32i -mabi=ilp32 -nostdlib -T "$PROGRAMS_DIR/link.ld" \
        "$PROGRAMS_DIR/npu_vector_example.c" -o "$PROGRAMS_DIR/npu_vector_example.elf"
fi

"$OBJCOPY" -O binary \
    "$PROGRAMS_DIR/npu_vector_example.elf" "$PROGRAMS_DIR/npu_vector_example.bin"

echo "Built: npu_vector_example.elf, npu_vector_example.bin"
echo "Done!"
