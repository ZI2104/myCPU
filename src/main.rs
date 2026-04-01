//! myCPU - RISC-V RV32I Simulator CLI
//!
//! Command-line interface for the myCPU RISC-V simulator.

use clap::{Parser, Subcommand};
use mycpu::cpu::csr::CsrRegister;
use mycpu::cpu::{Cpu, ExecutionModel};
use mycpu::debug::GdbServer;
use mycpu::interrupt::{Clint, Plic};
use mycpu::loader::ElfLoader;
use mycpu::memory::{Bus, Ram};
use mycpu::perf_report::PerfReport;
use mycpu::peripheral::{Lpu, Npu, Uart, VirtioBlock};
use mycpu::types::{Addr, RegIdx};
use mycpu::visualize::linux_fb_program;
use mycpu::visualize::start_visualize_server;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

const VIRTIO_SECTOR_SIZE: usize = 512;
const VIRTIO_MIN_SECTORS: usize = 1024;

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

        /// Print heartbeat every N instructions (0 = disabled)
        #[arg(long, default_value = "0")]
        heartbeat_every: u64,

        /// Enable verbose output
        #[arg(short, long)]
        verbose: bool,

        /// Generate performance report after execution
        #[arg(long)]
        perf_report: bool,

        /// Binary or ELF file to load
        #[arg(name = "FILE")]
        file: PathBuf,

        /// Optional VirtIO block disk image (raw)
        #[arg(long)]
        virtio_disk: Option<PathBuf>,
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

        /// Optional VirtIO block disk image (raw)
        #[arg(long)]
        virtio_disk: Option<PathBuf>,
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

        /// Optional VirtIO block disk image (raw)
        #[arg(long)]
        virtio_disk: Option<PathBuf>,
    },
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::Run {
            memory,
            pc,
            count,
            heartbeat_every,
            verbose,
            perf_report,
            file,
            virtio_disk,
        } => run_program(
            memory,
            &pc,
            count,
            heartbeat_every,
            verbose,
            perf_report,
            file,
            virtio_disk,
        ),
        Commands::Debug {
            port,
            memory,
            pc,
            file,
            virtio_disk,
        } => start_debug_server(port, memory, &pc, file, virtio_disk),
        Commands::Visualize {
            port,
            memory,
            pc,
            file,
            linux_fb_demo,
            warmup,
            virtio_disk,
        } => start_visualize(port, memory, &pc, file, linux_fb_demo, warmup, virtio_disk),
    }
}

/// Run a RISC-V program directly
fn run_program(
    memory_mb: usize,
    pc_str: &str,
    max_count: u64,
    heartbeat_every: u64,
    verbose: bool,
    show_perf_report: bool,
    file: PathBuf,
    virtio_disk: Option<PathBuf>,
) -> anyhow::Result<()> {
    init_logger(verbose);

    let requested_pc = parse_hex_address(pc_str)?;
    let mut bus = create_bus(memory_mb, virtio_disk.as_ref())?;

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

    if heartbeat_every > 0 {
        println!("Heartbeat enabled: every {} instructions", heartbeat_every);
    }

    let start_time = Instant::now();
    let instructions_executed = if heartbeat_every == 0 {
        cpu.run(max_count)?
    } else {
        run_with_heartbeat(&mut cpu, max_count, heartbeat_every)?
    };
    let elapsed = start_time.elapsed();

    println!("\n--- Execution complete ---");
    println!("Instructions executed: {}", instructions_executed);
    println!("Final PC: {}", cpu.pc());
    println!(
        "Elapsed: {:.3}s ({} instr/s)",
        elapsed.as_secs_f64(),
        if elapsed.as_secs_f64() > 0.0 {
            (instructions_executed as f64 / elapsed.as_secs_f64()) as u64
        } else {
            0
        }
    );

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

fn run_with_heartbeat(cpu: &mut Cpu, max_count: u64, heartbeat_every: u64) -> anyhow::Result<u64> {
    const XV6_CPU0_ADDR: u32 = 0x8001_34C4;
    const XV6_PROC_BASE: u32 = 0x8001_36E4;
    const XV6_TICKS_ADDR: u32 = 0x8002_4010;
    const XV6_MSCRATCH0_ADDR: u32 = 0x8000_B000;
    const XV6_TICKSLOCK_ADDR: u32 = 0x8001_66E4;
    const XV6_NPROC: u32 = 64;
    const XV6_PROC_STRIDE: u32 = 192;
    const PROC_STATE_OFFSET: u32 = 12;
    const CPU_PROC_OFFSET: u32 = 0;
    const CPU_NOFF_OFFSET: u32 = 60;
    const CPU_INTENA_OFFSET: u32 = 64;

    let mut count = 0u64;
    let mut last_hb_pc: Option<Addr> = None;
    let mut same_pc_streak = 0u64;

    while !cpu.is_halted() {
        if max_count > 0 && count >= max_count {
            break;
        }

        cpu.step()?;
        count += 1;

        if count % heartbeat_every == 0 {
            let csr = cpu.csr();
            let mip = csr.mip.read();
            let mie = csr.mie.read();
            let global_mie = csr.mstatus.mie();
            let sip = csr.sip.read();
            let sie = csr.sie.read();
            let scause = csr.scause.read();
            let sepc = csr.sepc.read();
            let global_sie = csr.sstatus.sie();
            let (mtip, msip) = cpu.bus().get_clint_interrupt_status();
            let (meip, _seip) = cpu.bus().get_plic_interrupt_status();
            let virtio_irq = cpu.bus().has_peripheral_interrupt("VirtIO-Block");
            let uart_irq = cpu.bus().has_peripheral_interrupt("UART");
            let (
                virtio_cmds,
                virtio_notifies,
                virtio_desc,
                virtio_irq_raised,
                virtio_irq_ack,
                virtio_desc_not_ready,
                virtio_desc_no_avail,
                virtio_desc_success,
                virtio_desc_error,
            ) = cpu
                .bus()
                .get_virtio_activity_counters()
                .unwrap_or((0, 0, 0, 0, 0, 0, 0, 0, 0));
            let cpu0_proc = cpu
                .bus()
                .read_word(Addr::new(XV6_CPU0_ADDR + CPU_PROC_OFFSET))
                .ok()
                .map(|w| w.raw())
                .unwrap_or(0);
            let cpu0_noff = cpu
                .bus()
                .read_word(Addr::new(XV6_CPU0_ADDR + CPU_NOFF_OFFSET))
                .ok()
                .map(|w| w.raw() as i32)
                .unwrap_or(-1);
            let cpu0_intena = cpu
                .bus()
                .read_word(Addr::new(XV6_CPU0_ADDR + CPU_INTENA_OFFSET))
                .ok()
                .map(|w| w.raw())
                .unwrap_or(0);
            let ticks = cpu
                .bus()
                .read_word(Addr::new(XV6_TICKS_ADDR))
                .ok()
                .map(|w| w.raw())
                .unwrap_or(0);
            let clint_mtime = cpu
                .bus()
                .read_word(Addr::new(0x0200_BFF8))
                .ok()
                .map(|w| w.raw())
                .unwrap_or(0);
            let clint_mtimecmp = cpu
                .bus()
                .read_word(Addr::new(0x0200_4000))
                .ok()
                .map(|w| w.raw())
                .unwrap_or(0);
            let mscratch = cpu.csr().mscratch.get();
            let scratch_interval = cpu
                .bus()
                .read_word(Addr::new(XV6_MSCRATCH0_ADDR + 20))
                .ok()
                .map(|w| w.raw())
                .unwrap_or(0);
            let tickslock_locked = cpu
                .bus()
                .read_word(Addr::new(XV6_TICKSLOCK_ADDR))
                .ok()
                .map(|w| w.raw())
                .unwrap_or(0);
            let tickslock_cpu = cpu
                .bus()
                .read_word(Addr::new(XV6_TICKSLOCK_ADDR + 8))
                .ok()
                .map(|w| w.raw())
                .unwrap_or(0);
            let proc0_state = cpu
                .bus()
                .read_word(Addr::new(XV6_PROC_BASE + PROC_STATE_OFFSET))
                .ok()
                .map(|w| w.raw())
                .unwrap_or(0);
            let proc0_pid = cpu
                .bus()
                .read_word(Addr::new(XV6_PROC_BASE + 32))
                .ok()
                .map(|w| w.raw())
                .unwrap_or(0);
            let proc0_ctx_ra = cpu
                .bus()
                .read_word(Addr::new(XV6_PROC_BASE + 52))
                .ok()
                .map(|w| w.raw())
                .unwrap_or(0);
            let proc0_state_cpu_view = cpu
                .read_word(Addr::new(XV6_PROC_BASE + PROC_STATE_OFFSET))
                .ok()
                .map(|w| w.raw())
                .unwrap_or(u32::MAX);
            let plic_pending0 = cpu
                .bus()
                .read_word(Addr::new(0x0C00_1000))
                .ok()
                .map(|w| w.raw())
                .unwrap_or(0);
            let plic_senable0 = cpu
                .bus()
                .read_word(Addr::new(0x0C00_2080))
                .ok()
                .map(|w| w.raw())
                .unwrap_or(0);
            let plic_sthreshold0 = cpu
                .bus()
                .read_word(Addr::new(0x0C20_1000))
                .ok()
                .map(|w| w.raw())
                .unwrap_or(0);
            let plic_sclaim_peek = cpu
                .bus()
                .read_word(Addr::new(0x0C20_1004))
                .ok()
                .map(|w| w.raw())
                .unwrap_or(0);
            let mut proc_runnable = 0u32;
            let mut proc_running = 0u32;
            let mut proc_sleeping = 0u32;
            let mut proc_runnable_locked = 0u32;
            let mut first_runnable_idx: i32 = -1;
            let mut first_runnable_lock_cpu = 0u32;
            for i in 0..XV6_NPROC {
                let proc_base = XV6_PROC_BASE + i * XV6_PROC_STRIDE;
                let state_addr = XV6_PROC_BASE + i * XV6_PROC_STRIDE + PROC_STATE_OFFSET;
                if let Ok(state) = cpu.bus().read_word(Addr::new(state_addr)) {
                    match state.raw() {
                        1 => proc_sleeping += 1,
                        2 => {
                            proc_runnable += 1;
                            if first_runnable_idx < 0 {
                                first_runnable_idx = i as i32;
                            }
                            let lock_word = cpu
                                .bus()
                                .read_word(Addr::new(proc_base))
                                .ok()
                                .map(|w| w.raw())
                                .unwrap_or(0);
                            if lock_word != 0 {
                                proc_runnable_locked += 1;
                            }
                            if first_runnable_idx == i as i32 {
                                first_runnable_lock_cpu = cpu
                                    .bus()
                                    .read_word(Addr::new(proc_base + 8))
                                    .ok()
                                    .map(|w| w.raw())
                                    .unwrap_or(0);
                            }
                        }
                        3 => proc_running += 1,
                        _ => {}
                    }
                }
            }
            let tp = cpu.registers().read(RegIdx::new(4)).raw();
            let pc = cpu.pc();

            if Some(pc) == last_hb_pc {
                same_pc_streak += 1;
            } else {
                same_pc_streak = 0;
                last_hb_pc = Some(pc);
            }

            println!(
                "[hb] step={} pc={} priv={} tp=0x{:08x} mstatus.mie={} sstatus.sie={} mip=0x{:08x} mie=0x{:08x} sip=0x{:08x} sie=0x{:08x} scause=0x{:08x} sepc=0x{:08x} mtip={} msip={} meip={} virtio_irq={} uart_irq={} v_cmd={} v_notify={} v_desc={} v_irq_raise={} v_irq_ack={} v_desc_not_ready={} v_desc_no_avail={} v_desc_ok={} v_desc_err={} plic_pending0=0x{:08x} plic_senable0=0x{:08x} plic_sth=0x{:08x} plic_sclaim={} cpu0_proc=0x{:08x} cpu0_noff={} cpu0_intena={} ticks={} mtime={} mtimecmp={} mscratch=0x{:08x} scratch5={} tickslock_locked={} tickslock_cpu=0x{:08x} p0_state={} p0_state_cpu={} p0_pid={} p0_ctx_ra=0x{:08x} p_run={} p_run_locked={} p_run0_idx={} p_run0_lock_cpu=0x{:08x} p_running={} p_sleep={} pc_streak={}",
                count,
                pc,
                cpu.privilege(),
                tp,
                if global_mie { 1 } else { 0 },
                if global_sie { 1 } else { 0 },
                mip,
                mie,
                sip,
                sie,
                scause,
                sepc,
                if mtip { 1 } else { 0 },
                if msip { 1 } else { 0 },
                if meip { 1 } else { 0 },
                if virtio_irq { 1 } else { 0 },
                if uart_irq { 1 } else { 0 },
                virtio_cmds,
                virtio_notifies,
                virtio_desc,
                virtio_irq_raised,
                virtio_irq_ack,
                virtio_desc_not_ready,
                virtio_desc_no_avail,
                virtio_desc_success,
                virtio_desc_error,
                plic_pending0,
                plic_senable0,
                plic_sthreshold0,
                plic_sclaim_peek,
                cpu0_proc,
                cpu0_noff,
                cpu0_intena,
                ticks,
                clint_mtime,
                clint_mtimecmp,
                mscratch,
                scratch_interval,
                tickslock_locked,
                tickslock_cpu,
                proc0_state,
                proc0_state_cpu_view,
                proc0_pid,
                proc0_ctx_ra,
                proc_runnable,
                proc_runnable_locked,
                first_runnable_idx,
                first_runnable_lock_cpu,
                proc_running,
                proc_sleeping,
                same_pc_streak
            );
            std::io::stdout().flush().ok();
        }
    }

    Ok(count)
}

/// Start GDB debug server
fn start_debug_server(
    port: u16,
    memory_mb: usize,
    pc_str: &str,
    file: PathBuf,
    virtio_disk: Option<PathBuf>,
) -> anyhow::Result<()> {
    init_logger(true);

    let requested_pc = parse_hex_address(pc_str)?;
    let mut bus = create_bus(memory_mb, virtio_disk.as_ref())?;

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
fn create_bus(memory_mb: usize, virtio_disk: Option<&PathBuf>) -> anyhow::Result<Bus> {
    let mut bus = Bus::new();
    let memory_size = memory_mb * 1024 * 1024;
    let ram = Ram::new(memory_size);
    bus.attach_memory(Addr::new(0x80000000), ram, "Main RAM");

    // QEMU-virt compatible interrupt controllers required by most RV32 OS kernels.
    bus.attach_peripheral(Clint::new());
    bus.attach_peripheral(Plic::new());

    bus.attach_peripheral(Npu::new());
    bus.attach_peripheral(Lpu::new());

    let virtio_block = if let Some(path) = virtio_disk {
        let image = std::fs::read(path)?;
        let sectors = image
            .len()
            .div_ceil(VIRTIO_SECTOR_SIZE)
            .max(VIRTIO_MIN_SECTORS);
        let mut block = VirtioBlock::with_disk_sectors(sectors);
        block
            .load_disk_image(&image)
            .map_err(|e| anyhow::anyhow!("failed to load virtio disk image: {}", e))?;
        println!(
            "Loaded VirtIO disk image: {} ({} bytes, {} sectors)",
            path.display(),
            image.len(),
            sectors
        );
        block
    } else {
        VirtioBlock::new()
    };

    bus.attach_peripheral(virtio_block);
    Ok(bus)
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
    virtio_disk: Option<PathBuf>,
) -> anyhow::Result<()> {
    init_logger(true);

    let requested_pc = parse_hex_address(pc_str)?;
    let mut bus = create_bus(memory_mb, virtio_disk.as_ref())?;

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
