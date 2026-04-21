use std::process::Command;
use std::time::{Duration, Instant};
use std::{net::TcpStream, thread};

#[test]
fn demo_gdb_starts_and_accepts_connection() -> Result<(), Box<dyn std::error::Error>> {
    // To avoid surprising CI/local runs, this test is opt-in. Set RUN_DEMO_TESTS=1 to enable.
    if std::env::var("RUN_DEMO_TESTS").is_err() {
        eprintln!(
            "Skipping demo_gdb_starts_and_accepts_connection: set RUN_DEMO_TESTS=1 to enable"
        );
        return Ok(());
    }

    let elf = "tests/programs/fib.elf";
    assert!(
        std::path::Path::new(elf).exists(),
        "Demo ELF not found: {}. Build demo assets first: Windows => scripts/build_demo_programs.ps1, Unix/WSL => tests/programs/build.sh",
        elf
    );

    // Spawn `cargo run -- debug tests/programs/fib.elf` which should start a GDB server on port 1234
    let mut child = Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .arg("--")
        .arg("debug")
        .arg(elf)
        .spawn()?;

    // Wait for the process to open the GDB port
    let start = Instant::now();
    let timeout = Duration::from_secs(12);
    let mut connected = false;
    while start.elapsed() < timeout {
        match TcpStream::connect(("127.0.0.1", 1234)) {
            Ok(_) => {
                connected = true;
                break;
            }
            Err(_) => {
                thread::sleep(Duration::from_millis(200));
            }
        }
    }

    // Clean up the child process
    let _ = child.kill();
    let _ = child.wait();

    assert!(
        connected,
        "GDB server did not accept connections on 127.0.0.1:1234 within timeout"
    );
    Ok(())
}
