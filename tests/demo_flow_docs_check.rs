use std::path::Path;

const DEMO_GUIDE: &str = include_str!("../docs/guides/DEMO_GUIDE.md");

#[test]
fn guide_contains_demo_build_steps() {
    assert!(
        DEMO_GUIDE.contains("scripts\\build_demo_programs.ps1"),
        "DEMO_GUIDE should include Windows demo build script step"
    );
    assert!(
        DEMO_GUIDE.contains("bash tests/programs/build.sh"),
        "DEMO_GUIDE should include Unix/WSL demo build step"
    );
}

#[test]
fn ecall_exit_flag_position_is_documented_correctly() {
    let correct = "cargo run --release -- run --ecall-exit tests/programs/hello.bin";
    let wrong = "cargo run --release --ecall-exit -- run tests/programs/hello.bin";

    assert!(
        DEMO_GUIDE.contains(correct),
        "DEMO_GUIDE must contain the correct --ecall-exit command placement"
    );
    assert!(
        DEMO_GUIDE.contains(wrong),
        "DEMO_GUIDE should explicitly call out the incorrect flag placement for clarity"
    );
}

#[test]
fn demo_commands_are_present() {
    assert!(
        DEMO_GUIDE.contains("cargo run --release -- debug tests/programs/fib.elf"),
        "GDB demo command should exist in DEMO_GUIDE"
    );
    assert!(
        DEMO_GUIDE.contains("cargo test --lib difftest"),
        "DiffTest module validation command should exist in DEMO_GUIDE"
    );
}

#[test]
fn demo_source_files_exist() {
    for file in [
        "tests/programs/hello.s",
        "tests/programs/fib.s",
        "tests/programs/test.s",
        "tests/programs/build.sh",
        "scripts/build_demo_programs.ps1",
    ] {
        assert!(
            Path::new(file).exists(),
            "Required demo file missing: {file}"
        );
    }
}

#[test]
fn built_demo_artifacts_exist_when_enabled() {
    if std::env::var("CHECK_DEMO_ARTIFACTS").is_err() {
        eprintln!(
            "Skipping artifact existence check: set CHECK_DEMO_ARTIFACTS=1 after running build_demo_programs script"
        );
        return;
    }

    for artifact in [
        "tests/programs/hello.bin",
        "tests/programs/fib.elf",
        "tests/programs/test.elf",
    ] {
        assert!(
            Path::new(artifact).exists(),
            "Expected built artifact not found: {artifact}"
        );
    }
}
