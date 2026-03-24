//! myCPU - RISC-V RV32I Simulator CLI
//!
//! Command-line interface for the myCPU RISC-V simulator.

use clap::Parser;
use mycpu::cpu::Cpu;
use mycpu::memory::{Bus, Ram};
use mycpu::types::Addr;
use std::path::PathBuf;

/// myCPU - A RISC-V RV32I Instruction Set Simulator
#[derive(Parser, Debug)]
#[command(name = "mycpu")]
#[command(author = "myCPU Team")]
#[command(version)]
#[command(about = "A RISC-V RV32I simulator", long_about = None)]
struct Args {
    /// Memory size in MB
    #[arg(short, long, default_value = "16")]
    memory: usize,

    /// Starting PC address (hex)
    #[arg(short, long, default_value = "0x80000000")]
    pc: String,

    /// Number of instructions to execute (0 = unlimited)
    #[arg(short, long, default_value = "0")]
    count: u64,

    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Binary file to load
    #[arg(name = "FILE")]
    file: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize logger
    if args.verbose {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug"))
            .init();
    } else {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .init();
    }

    // Parse PC address
    let start_pc = parse_hex_address(&args.pc)?;

    // Create system bus
    let mut bus = Bus::new();

    // Attach RAM
    let memory_size = args.memory * 1024 * 1024; // Convert MB to bytes
    let ram = Ram::new(memory_size);
    bus.attach_memory(Addr::new(0x80000000), ram, "Main RAM");

    // Load binary file if provided
    if let Some(ref file_path) = args.file {
        load_binary(&mut bus, file_path, start_pc)?;
        println!("Loaded binary from: {}", file_path.display());
    }

    // Print memory map
    bus.print_memory_map();

    // Create CPU
    let mut cpu = Cpu::with_pc(bus, start_pc);

    println!("\nmyCPU RISC-V Simulator v{}", mycpu::VERSION);
    println!("Starting PC: {}", start_pc);
    println!("Memory size: {} MB", args.memory);
    println!("Max instructions: {}", if args.count == 0 { "unlimited".to_string() } else { args.count.to_string() });
    println!("\n--- Starting execution ---\n");

    // Run the CPU
    let instructions_executed = cpu.run(args.count)?;

    println!("\n--- Execution complete ---");
    println!("Instructions executed: {}", instructions_executed);
    println!("Final PC: {}", cpu.pc());

    // Print final register state
    if args.verbose {
        println!("\nFinal register state:");
        println!("{}", cpu.registers());
    }

    Ok(())
}

/// Parse a hex address string.
fn parse_hex_address(s: &str) -> anyhow::Result<Addr> {
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    let value = u32::from_str_radix(s, 16)?;
    Ok(Addr::new(value))
}

/// Load a binary file into memory.
fn load_binary(bus: &mut Bus, path: &PathBuf, load_addr: Addr) -> anyhow::Result<()> {
    let data = std::fs::read(path)?;
    bus.write_bytes(load_addr, &data)?;
    println!("Loaded {} bytes at {}", data.len(), load_addr);
    Ok(())
}
