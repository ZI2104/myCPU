NPU Integration: Debug Summary

Date: 2026-04-02

Summary
-------
This document records the key findings while implementing and testing the NPU vector-mode integration.

Toolchain
---------
- We use the `riscv64-unknown-elf-gcc` driver with RV32 flags to build RV32 ELFs:
  - Example flags: `-march=rv32i -mabi=ilp32 -nostdlib -T link.ld`
- Rationale: Using the riscv64 driver with explicit RV32 arguments avoids ABI mismatch between assembler and linker and removes the need to install a separate `riscv32-unknown-elf-*` toolchain for now.

Issue encountered
-----------------
- Initial integration test failed: CPU encountered an instruction page fault during ELF execution and PC moved to 0.
- Root cause analysis:
  - The C-based guest example pulled in C runtime/linker differences that led to unexpected behavior when assembling/linking with mismatched toolchain pieces.
  - Using the `riscv64` driver without explicit march/mabi or mixing assembler/linker prefixes caused ELF/ABI mismatches in some cases.

Fixes and mitigations
---------------------
1. Created a minimal assembly guest `tests/programs/npu_vector_example.s` that directly writes the NPU descriptor and notifies the NPU, avoiding C runtime complexity.
2. Updated `tests/programs/build.sh` to prefer assembly (`.s`) when present and otherwise fall back to C compilation.
3. Made debug logging in `tests/npu_elf_integration.rs` conditional on `MYCPU_NPU_DEBUG` environment variable so tests are quiet by default.

Reproduction and verification
----------------------------
- Build (in WSL) and run integration test:

```bash
cd /mnt/d/code/myCPU/tests/programs
riscv64-unknown-elf-gcc -march=rv32i -mabi=ilp32 -nostdlib -T link.ld npu_vector_example.s -o npu_vector_example.elf
riscv64-unknown-elf-objcopy -O binary npu_vector_example.elf npu_vector_example.bin

# Run integration test from project root (Windows or WSL cargo)
cargo test --test npu_elf_integration -- --nocapture
```

- To enable verbose debugging for this test, set the environment variable `MYCPU_NPU_DEBUG=1` before running the test.

Key successful log excerpts
---------------------------
- NPU writes observed during successful run (example):
```
[NPU] REG_DESC_ADDR_LOW <- 0x00000090
[NPU] REG_DESC_ADDR_LOW <- 0x80000090
[NPU] REG_DESC_NOTIFY <- 1 (pending_desc_notify set)
```
- Integration test exit: `test test_npu_elf_end_to_end ... ok`

Notes and next steps
--------------------
- If we later need full RV64 support, add CI step and toolchain variants for `riscv32-unknown-elf-*` and `riscv64-unknown-elf-*` and unify `build.sh` detection logic.
- Consider converting the optional debug printing to a test feature flag if we want compile-time control instead of environment variables.

Authors
-------
- Automated assistant + repo maintainer actions
