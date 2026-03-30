//! myCPU - RISC-V RV32I Simulator CLI
//!
//! Command-line interface for the myCPU RISC-V simulator.

use clap::{Parser, Subcommand};
use mycpu::cpu::Cpu;
use mycpu::debug::GdbServer;
use mycpu::loader::ElfLoader;
use mycpu::memory::{Bus, Ram};
use mycpu::peripheral::Uart;
use mycpu::types::Addr;
use std::io::Write;
use std::path::PathBuf;

/// myCPU - A RISC-V RV32I Instruction Set Simulator
#[derive(Parser, Debug)]
#[command(name = "mycpu")]
#[command(author = "myCPU Team")]
#[command(version)]
#[command(about = "A RISC-V RV32I simulator", long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run a RISC-V program
    Run {
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

        /// Binary or ELF file to load
        #[arg(name = "FILE")]
        file: PathBuf,
    },

    /// Start GDB debug server
    Debug {
        /// GDB server port
        #[arg(short, long, default_value = "1234")]
        port: u16,

        /// Memory size in MB
        #[arg(short, long, default_value = "16")]
        memory: usize,

        /// Starting PC address (hex)
        #[arg(short, long, default_value = "0x80000000")]
        pc: String,

        /// Binary or ELF file to load
        #[arg(name = "FILE")]
        file: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::Run {
            memory,
            pc,
            count,
            verbose,
            file,
        } => run_program(memory, &pc, count, verbose, file),
        Commands::Debug {
            port,
            memory,
            pc,
            file,
        } => start_debug_server(port, memory, &pc, file),
    }
}

/// Run a RISC-V program directly
fn run_program(
    memory_mb: usize,
    pc_str: &str,
    max_count: u64,
    verbose: bool,
    file: PathBuf,
) -> anyhow::Result<()> {
    init_logger(verbose);

    let start_pc = parse_hex_address(pc_str)?;
    let mut bus = create_bus(memory_mb);

    // Attach UART for output
    attach_stdout_uart(&mut bus);

    load_file(&mut bus, &file, start_pc)?;
    bus.print_memory_map();

    let mut cpu = Cpu::with_pc(bus, start_pc);

    println!("\nmyCPU RISC-V Simulator v{}", mycpu::VERSION);
    println!("Starting PC: {}", start_pc);
    println!("Memory size: {} MB", memory_mb);
    println!(
        "Max instructions: {}",
        if max_count == 0 {
            "unlimited".to_string()
        } else {
            max_count.to_string()
        }
    );
    println!("\n--- Starting execution ---\n");

    let instructions_executed = cpu.run(max_count)?;

    println!("\n--- Execution complete ---");
    println!("Instructions executed: {}", instructions_executed);
    println!("Final PC: {}", cpu.pc());

    if verbose {
        println!("\nFinal register state:");
        println!("{}", cpu.registers());
    }

    Ok(())
}

/// Start GDB debug server
fn start_debug_server(
    port: u16,
    memory_mb: usize,
    pc_str: &str,
    file: PathBuf,
) -> anyhow::Result<()> {
    init_logger(true);

    let start_pc = parse_hex_address(pc_str)?;
    let mut bus = create_bus(memory_mb);

    // Attach UART for output
    attach_stdout_uart(&mut bus);

    load_file(&mut bus, &file, start_pc)?;

    println!("myCPU GDB Debug Server v{}", mycpu::VERSION);
    println!("Listening on port {}", port);
    println!("Program loaded: {}", file.display());
    println!("Entry point: {}", start_pc);
    println!("\nConnect with: riscv32-unknown-elf-gdb -ex 'target remote localhost:{}'", port);

    // Create a tokio runtime for the async GDB server
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let server = GdbServer::new(port);
        server.run().await.map_err(|e| anyhow::anyhow!("{}", e))
    })
}

/// Parse a hex address string.
fn parse_hex_address(s: &str) -> anyhow::Result<Addr> {
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    let value = u32::from_str_radix(s, 16)?;
    Ok(Addr::new(value))
}

/// Initialize the logger
fn init_logger(verbose: bool) {
    let filter = if verbose { "debug" } else { "info" };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(filter)).init();
}

/// Create system bus with RAM
fn create_bus(memory_mb: usize) -> Bus {
    let mut bus = Bus::new();
    let memory_size = memory_mb * 1024 * 1024;
    let ram = Ram::new(memory_size);
    bus.attach_memory(Addr::new(0x80000000), ram, "Main RAM");
    bus
}

/// Attach UART that outputs to stdout
fn attach_stdout_uart(bus: &mut Bus) {
    let mut uart = Uart::with_base(Addr::new(0x1000_0000));
    uart.set_output_callback(Box::new(|byte| {
        print!("{}", byte as char);
        std::io::stdout().flush().ok();
    }));
    bus.attach_peripheral(uart);
}

/// Load a binary or ELF file into memory
fn load_file(bus: &mut Bus, path: &PathBuf, load_addr: Addr) -> anyhow::Result<()> {
    let data = std::fs::read(path)?;

    // Check if it's an ELF file (magic: 0x7F 'E' 'L' 'F')
    if data.len() >= 4 && &data[0..4] == &[0x7F, b'E', b'L', b'F'] {
        let loader = ElfLoader::from_bytes(data)?;
        let entry = loader.entry_point();
        println!("Loading ELF file: {} segments", loader.segment_count());
        loader.load_into(bus)?;
        println!("ELF entry point: {}", entry);
        for seg in loader.segments() {
            println!(
                "  Segment: vaddr=0x{:08x} size={} flags={:03b}",
                seg.vaddr, seg.memsz, seg.flags
            );
        }
    } else {
        // Raw binary
        bus.write_bytes(load_addr, &data)?;
        println!("Loaded {} bytes at {}", data.len(), load_addr);
    }

    Ok(())
}
