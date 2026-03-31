//! myCPU - RISC-V RV32I Simulator CLI
//!
//! Command-line interface for the myCPU RISC-V simulator.

use clap::{Parser, Subcommand};
use mycpu::cpu::{Cpu, ExecutionModel};
use mycpu::debug::GdbServer;
use mycpu::loader::ElfLoader;
use mycpu::memory::{Bus, Ram};
use mycpu::perf_report::PerfReport;
use mycpu::peripheral::{Lpu, Npu, Uart};
use mycpu::types::Addr;
use mycpu::visualize::linux_fb_program;
use mycpu::visualize::start_visualize_server;
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

        /// Generate performance report after execution
        #[arg(long)]
        perf_report: bool,

        /// Binary or ELF file to load
        #[arg(name = "FILE")]
        file: PathBuf,
    },

    /// Start GDB debug server
    Debug {
        /// GDB server port
        #[arg(short = 'g', long, default_value = "1234")]
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

    /// Start visualization server
    Visualize {
        /// WebSocket server port
        #[arg(short = 'w', long, default_value = "8080")]
        port: u16,

        /// Memory size in MB
        #[arg(short, long, default_value = "16")]
        memory: usize,

        /// Starting PC address (hex)
        #[arg(short, long, default_value = "0x80000000")]
        pc: String,

        /// Binary or ELF file to load (optional)
        #[arg(name = "FILE")]
        file: Option<PathBuf>,

        /// Preload built-in Linux framebuffer writer program when FILE is omitted
        #[arg(long, default_value_t = false)]
        linux_fb_demo: bool,

        /// Warm up CPU by executing N instructions before opening WebSocket server
        #[arg(long, default_value_t = 0)]
        warmup: u64,
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
            perf_report,
            file,
        } => run_program(memory, &pc, count, verbose, perf_report, file),
        Commands::Debug {
            port,
            memory,
            pc,
            file,
        } => start_debug_server(port, memory, &pc, file),
        Commands::Visualize {
            port,
            memory,
            pc,
            file,
            linux_fb_demo,
            warmup,
        } => start_visualize(port, memory, &pc, file, linux_fb_demo, warmup),
    }
}

/// Run a RISC-V program directly
fn run_program(
    memory_mb: usize,
    pc_str: &str,
    max_count: u64,
    verbose: bool,
    show_perf_report: bool,
    file: PathBuf,
) -> anyhow::Result<()> {
    init_logger(verbose);

    let requested_pc = parse_hex_address(pc_str)?;
    let mut bus = create_bus(memory_mb);

    // Attach UART for output
    attach_stdout_uart(&mut bus);

    let file_entry = load_file(&mut bus, &file, requested_pc)?;
    let start_pc = file_entry.unwrap_or(requested_pc);
    bus.print_memory_map();

    let mut cpu = Cpu::with_pc(bus, start_pc);

    println!("\nmyCPU RISC-V Simulator v{}", mycpu::VERSION);
    println!("Starting PC: {}", start_pc);
    if start_pc != requested_pc {
        println!(
            "Requested PC {} overridden by ELF entry {}",
            requested_pc, start_pc
        );
    }
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

    // Generate performance report if requested
    if show_perf_report {
        let report = PerfReport::from_collector(cpu.perf_collector());
        println!("{}", report);
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

    let requested_pc = parse_hex_address(pc_str)?;
    let mut bus = create_bus(memory_mb);

    // Attach UART for output
    attach_stdout_uart(&mut bus);

    let file_entry = load_file(&mut bus, &file, requested_pc)?;
    let start_pc = file_entry.unwrap_or(requested_pc);

    let cpu = Cpu::with_pc(bus, start_pc);

    println!("myCPU GDB Debug Server v{}", mycpu::VERSION);
    println!("Listening on port {}", port);
    println!("Program loaded: {}", file.display());
    println!("Entry point: {}", start_pc);
    if start_pc != requested_pc {
        println!(
            "Requested PC {} overridden by ELF entry {}",
            requested_pc, start_pc
        );
    }
    println!(
        "\nConnect with: riscv32-unknown-elf-gdb -ex 'target remote localhost:{}'",
        port
    );

    // Create a tokio runtime for the async GDB server
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let server = GdbServer::with_cpu(port, cpu);
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
    bus.attach_peripheral(Npu::new());
    bus.attach_peripheral(Lpu::new());
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
fn load_file(bus: &mut Bus, path: &PathBuf, load_addr: Addr) -> anyhow::Result<Option<Addr>> {
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
        Ok(Some(entry))
    } else {
        // Raw binary
        bus.write_bytes(load_addr, &data)?;
        println!("Loaded {} bytes at {}", data.len(), load_addr);
        Ok(None)
    }
}

/// Start visualization server
fn start_visualize(
    port: u16,
    memory_mb: usize,
    pc_str: &str,
    file: Option<PathBuf>,
    linux_fb_demo: bool,
    warmup: u64,
) -> anyhow::Result<()> {
    init_logger(true);

    let requested_pc = parse_hex_address(pc_str)?;
    let mut bus = create_bus(memory_mb);

    // Attach UART for output
    attach_stdout_uart(&mut bus);

    if linux_fb_demo {
        if file.is_none() {
            let bytes = linux_fb_program::load_demo_program(&mut bus, requested_pc)?;
            println!(
                "Loaded built-in Linux framebuffer demo program: {} bytes at {}",
                bytes, requested_pc
            );
            println!(
                "Framebuffer preset: addr=0x{:08x} size={}x{} format={}",
                linux_fb_program::LINUX_FB_ADDR,
                linux_fb_program::LINUX_FB_WIDTH,
                linux_fb_program::LINUX_FB_HEIGHT,
                linux_fb_program::LINUX_FB_FORMAT
            );
        } else {
            println!(
                "--linux-fb-demo is ignored because FILE was provided; using external program"
            );
        }
    }

    let start_pc = if let Some(path) = file.as_ref() {
        let file_entry = load_file(&mut bus, path, requested_pc)?;
        file_entry.unwrap_or(requested_pc)
    } else {
        requested_pc
    };

    println!("myCPU Visualization Server v{}", mycpu::VERSION);
    println!("WebSocket port: {}", port);
    if let Some(path) = file.as_ref() {
        println!("Program loaded: {}", path.display());
    } else {
        println!("Program loaded: <none>");
        println!("Demo mode: empty RAM + framebuffer demo commands enabled");
    }
    println!("Entry point: {}", start_pc);
    if start_pc != requested_pc {
        println!(
            "Requested PC {} overridden by ELF entry {}",
            requested_pc, start_pc
        );
    }
    println!("\nConnect with WebSocket client at ws://127.0.0.1:{}", port);
    println!("Or open frontend/index.html in browser");

    // Create CPU with pipeline (for visualization)
    let mut cpu = mycpu::cpu::pipeline::PipelineCpu::with_pc(bus, start_pc);

    if warmup > 0 {
        let executed = cpu.run(warmup)?;
        println!("Warmup executed {} instruction steps", executed);
    }

    // Create tokio runtime and start server
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        start_visualize_server(cpu, port)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    })
}
