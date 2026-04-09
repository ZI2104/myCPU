#!/usr/bin/env bash
set -euo pipefail

# Run official riscv-tests ISA tests with myCPU DiffTest + QEMU in CI.
# Usage: ./scripts/run_riscv_tests.sh [max_tests]

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TP_DIR="$ROOT/third_party/riscv-tests"
ARTIFACT_DIR="$ROOT/artifacts/riscv-tests"
mkdir -p "$ARTIFACT_DIR"

NUM_LIMIT=${1:-0} # 0 = no limit

if ! [[ "$NUM_LIMIT" =~ ^[0-9]+$ ]]; then
  echo "Error: max_tests must be a non-negative integer, got '$NUM_LIMIT'." >&2
  exit 1
fi

# Choose CROSS prefix
if command -v riscv32-unknown-elf-gcc >/dev/null 2>&1; then
  CROSS=riscv32-unknown-elf-
elif command -v riscv64-unknown-elf-gcc >/dev/null 2>&1; then
  # riscv64 driver can emit RV32 binaries with -march/mabi flags
  CROSS=riscv64-unknown-elf-
else
  echo "Error: No riscv cross-compiler found. Install riscv32-unknown-elf-gcc or riscv64-unknown-elf-gcc." >&2
  exit 1
fi

# Choose QEMU binary
if command -v qemu-system-riscv32 >/dev/null 2>&1; then
  QEMU=qemu-system-riscv32
elif command -v qemu-system-riscv >/dev/null 2>&1; then
  QEMU=qemu-system-riscv
else
  echo "Error: qemu-system-riscv32 (or qemu-system-riscv) not found. Install qemu-system-riscv32." >&2
  exit 1
fi

# Clone riscv-tests if needed
if [ ! -d "$TP_DIR" ]; then
  echo "Cloning riscv-tests into $TP_DIR..."
  git clone --depth=1 https://github.com/riscv/riscv-tests.git "$TP_DIR"
fi

cd "$TP_DIR/isa" || exit 1

# Build tests (best-effort)
echo "Building riscv-tests (CROSS=$CROSS)..."
if ! make -j"$(nproc)" CROSS="$CROSS"; then
  echo "make failed; attempting fallback with CROSS=riscv32-unknown-elf-..." >&2
  if ! make -j"$(nproc)" CROSS="riscv32-unknown-elf-"; then
    echo "riscv-tests build failed; aborting." >&2
    exit 1
  fi
fi

# Find ELF files built by riscv-tests
ELFS=( $(find . -type f -name "*.elf" | sort) )
if [ ${#ELFS[@]} -eq 0 ]; then
  echo "No ELF files found under $TP_DIR/isa. Build likely failed." >&2
  exit 1
fi

count=0
for rel in "${ELFS[@]}"; do
  elf_path="$TP_DIR/isa/$rel"
  elf_name=$(basename "$elf_path")
  test_name="${elf_name%.elf}"

  echo "\n=== Running test: $test_name ==="

  # Start QEMU with GDB stub on port 1234, paused
  echo "Starting QEMU: $QEMU -M virt -nographic -bios none -kernel $elf_path -S -s"
  $QEMU -M virt -nographic -bios none -kernel "$elf_path" -S -s > /dev/null 2>&1 &
  QEMU_PID=$!

  # Ensure we kill QEMU on exit
  trap 'kill ${QEMU_PID} >/dev/null 2>&1 || true' ERR EXIT

  # Wait for GDB port to be open (tcp/1234)
  echo "Waiting for QEMU gdb stub on 127.0.0.1:1234..."
  timeout=15
  waited=0
  while ! (echo > /dev/tcp/127.0.0.1/1234) 2>/dev/null; do
    sleep 0.2
    waited=$((waited+1))
    if [ $waited -gt $((timeout*5)) ]; then
      echo "Timed out waiting for QEMU gdb stub" >&2
      kill ${QEMU_PID} >/dev/null 2>&1 || true
      exit 1
    fi
  done

  # Run myCPU in difftest mode, connect to QEMU gdb stub
  JSON_OUT="$ARTIFACT_DIR/${test_name}.perf.json"
  mkdir -p "$(dirname "$JSON_OUT")"
  echo "Running myCPU (difftest) for $elf_name ..."

  pushd "$ROOT" >/dev/null
  # Set env so mycpu writes JSON perf report for CI
  export MYCPU_PERF_JSON="$JSON_OUT"
  set -x
  cargo run --release --features difftest -- run "$elf_path" --perf_report
  rc=$?
  set +x
  popd >/dev/null

  # Kill QEMU
  kill ${QEMU_PID} >/dev/null 2>&1 || true
  wait ${QEMU_PID} 2>/dev/null || true
  trap - ERR EXIT

  if [ $rc -ne 0 ]; then
    echo "Test $test_name failed (exit $rc)" >&2
    exit $rc
  fi

  echo "Test $test_name passed (perf JSON: $JSON_OUT)"

  count=$((count+1))
  if [ "$NUM_LIMIT" -ne 0 ] && [ $count -ge $NUM_LIMIT ]; then
    echo "Reached requested test limit: $NUM_LIMIT"
    break
  fi

done

echo "\nAll selected riscv-tests completed successfully. Total: $count"
exit 0
