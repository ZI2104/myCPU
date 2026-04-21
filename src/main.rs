//! myCPU - RISC-V RV32I Simulator CLI
//!
//! Command-line interface for the myCPU RISC-V simulator.

use clap::{Parser, Subcommand, ValueEnum};
use mycpu::cpu::csr::CsrRegister;
use mycpu::cpu::{Cpu, ExecutionModel};
use mycpu::debug::GdbServer;
use mycpu::interrupt::{Clint, Plic};
use mycpu::loader::ElfLoader;
use mycpu::memory::{Bus, Ram};
use mycpu::perf::PerfReport;
use mycpu::peripheral::{Gpu, InputDevice, Lpu, Npu, Tpu, Uart, VirtioBlock};
use mycpu::types::{Addr, RegIdx, Word};
use mycpu::visualize::linux_fb_program;
use mycpu::visualize::start_visualize_server_with_initial_pc;
use std::env;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

const VIRTIO_SECTOR_SIZE: usize = 512;
const VIRTIO_MIN_SECTORS: usize = 1024;
const OPENSBI_DYNAMIC_INFO_ADDR: u32 = 0x87EF_0000;
const OPENSBI_DYNAMIC_INFO_MAGIC: u32 = 0x4942_534F; // 'OSBI'
const OPENSBI_DYNAMIC_INFO_VERSION_2: u32 = 0x2;
const OPENSBI_DYNAMIC_INFO_NEXT_MODE_S: u32 = 0x1;
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
const CLINT_MTIME_ADDR: u32 = 0x0200_BFF8;
const CLINT_MTIMECMP_ADDR: u32 = 0x0200_4000;
const PLIC_PENDING0_ADDR: u32 = 0x0C00_1000;
const PLIC_SENABLE0_ADDR: u32 = 0x0C00_2080;
const PLIC_STHRESHOLD0_ADDR: u32 = 0x0C20_1000;
const PLIC_SCLAIM_PEEK_ADDR: u32 = 0x0C20_1004;

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

#[allow(clippy::large_enum_variant)]
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

        /// Heartbeat output mode: compact (default) or diagnostic (verbose)
        #[arg(long, value_enum, default_value = "compact")]
        heartbeat_mode: HeartbeatMode,

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

        /// Inject host string into UART RX FIFO (supports escapes like \\n, \\r, \\t)
        #[arg(long)]
        uart_script: Option<String>,

        /// Start injecting UART script at this instruction count
        #[arg(long, default_value = "0")]
        uart_inject_at: u64,

        /// Inject one UART byte every N instructions
        #[arg(long, default_value = "20000")]
        uart_inject_every: u64,

        /// UART injection trigger mode: by instruction step or shell prompt output
        #[arg(long, value_enum, default_value = "step")]
        uart_inject_trigger: UartInjectTrigger,

        /// Inject host input events into Input MMIO peripheral.
        ///
        /// Script format examples:
        /// - "right:down;right:up;a:down;a:up"
        /// - "up down\nup up\nclear"
        #[arg(long)]
        input_script: Option<String>,

        /// Start injecting input script at this instruction count
        #[arg(long, default_value = "0")]
        input_inject_at: u64,

        /// Inject one input action every N instructions
        #[arg(long, default_value = "500000")]
        input_inject_every: u64,

        /// Enable Linux boot context injection (hartid/dtb/bootargs)
        #[arg(long, default_value_t = false)]
        linux_boot: bool,

        /// Linux boot hartid (written to a0)
        #[arg(long, default_value = "0")]
        linux_hartid: u32,

        /// Optional Linux DTB blob path (loaded to guest memory; address written to a1)
        #[arg(long)]
        linux_dtb: Option<PathBuf>,

        /// Auto-generate a minimal Linux DTB when --linux-dtb is not provided
        #[arg(long, default_value_t = false)]
        linux_auto_dtb: bool,

        /// Guest memory address to place Linux DTB blob
        #[arg(long, default_value = "0x87f00000")]
        linux_dtb_addr: String,

        /// Optional Linux kernel bootargs string (NUL-terminated, written to guest memory)
        #[arg(long)]
        linux_bootargs: Option<String>,

        /// Guest memory address to place Linux bootargs string
        #[arg(long, default_value = "0x87ff0000")]
        linux_bootargs_addr: String,

        /// Optional SBI firmware image for Linux boot chain (entry used as start PC)
        #[arg(long)]
        linux_sbi: Option<PathBuf>,

        /// Guest memory address to place SBI firmware when image is raw binary
        #[arg(long, default_value = "0x80000000")]
        linux_sbi_addr: String,

        /// Guest memory address to place Linux payload image in SBI boot chain
        #[arg(long, default_value = "0x80200000")]
        linux_payload_addr: String,
        /// Treat ECALL with a7==93 as program exit
        #[arg(long, default_value_t = false)]
        ecall_exit: bool,
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

#[derive(Debug, Clone, Copy, ValueEnum)]
enum HeartbeatMode {
    Compact,
    Diagnostic,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum UartInjectTrigger {
    Step,
    Prompt,
}

#[derive(Debug, Clone)]
struct LinuxBootOptions {
    enabled: bool,
    hartid: u32,
    dtb: Option<PathBuf>,
    auto_dtb: bool,
    dtb_addr: String,
    bootargs: Option<String>,
    bootargs_addr: String,
    sbi: Option<PathBuf>,
    sbi_addr: String,
    payload_addr: String,
}

#[derive(Debug, Clone)]
struct RunProgramOptions {
    memory_mb: usize,
    pc: String,
    max_count: u64,
    heartbeat_every: u64,
    heartbeat_mode: HeartbeatMode,
    verbose: bool,
    show_perf_report: bool,
    file: PathBuf,
    virtio_disk: Option<PathBuf>,
    uart_script: Option<String>,
    uart_inject_at: u64,
    uart_inject_every: u64,
    uart_inject_trigger: UartInjectTrigger,
    input_script: Option<String>,
    input_inject_at: u64,
    input_inject_every: u64,
    linux: LinuxBootOptions,
    ecall_exit: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::Run {
            memory,
            pc,
            count,
            heartbeat_every,
            heartbeat_mode,
            verbose,
            perf_report,
            file,
            virtio_disk,
            uart_script,
            uart_inject_at,
            uart_inject_every,
            uart_inject_trigger,
            input_script,
            input_inject_at,
            input_inject_every,
            linux_boot,
            linux_hartid,
            linux_dtb,
            linux_auto_dtb,
            linux_dtb_addr,
            linux_bootargs,
            linux_bootargs_addr,
            linux_sbi,
            linux_sbi_addr,
            linux_payload_addr,
            ecall_exit,
        } => run_program(RunProgramOptions {
            memory_mb: memory,
            pc,
            max_count: count,
            heartbeat_every,
            heartbeat_mode,
            verbose,
            show_perf_report: perf_report,
            file,
            virtio_disk,
            uart_script,
            uart_inject_at,
            uart_inject_every,
            uart_inject_trigger,
            input_script,
            input_inject_at,
            input_inject_every,
            linux: LinuxBootOptions {
                enabled: linux_boot,
                hartid: linux_hartid,
                dtb: linux_dtb,
                auto_dtb: linux_auto_dtb,
                dtb_addr: linux_dtb_addr,
                bootargs: linux_bootargs,
                bootargs_addr: linux_bootargs_addr,
                sbi: linux_sbi,
                sbi_addr: linux_sbi_addr,
                payload_addr: linux_payload_addr,
            },
            ecall_exit,
        }),
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
fn run_program(opts: RunProgramOptions) -> anyhow::Result<()> {
    init_logger(opts.verbose);

    let requested_pc = parse_hex_address(&opts.pc)?;
    let mut bus = create_bus(opts.memory_mb, opts.virtio_disk.as_ref())?;
    let uart_prompt_ready = Arc::new(AtomicBool::new(false));

    // Attach UART for output
    attach_stdout_uart(&mut bus, Some(Arc::clone(&uart_prompt_ready)));

    let start_pc = if let Some(sbi_path) = opts.linux.sbi.as_ref() {
        if !opts.linux.enabled {
            return Err(anyhow::anyhow!(
                "--linux-sbi requires --linux-boot to be enabled"
            ));
        }

        let sbi_addr = parse_hex_address(&opts.linux.sbi_addr)?;
        let payload_addr = parse_hex_address(&opts.linux.payload_addr)?;
        let payload_size = load_raw_file_at(&mut bus, &opts.file, payload_addr)?;
        println!(
            "Linux boot chain: loaded payload {} ({} bytes) at {}",
            opts.file.display(),
            payload_size,
            payload_addr
        );

        let sbi_entry = load_file(&mut bus, sbi_path, sbi_addr)?;
        let sbi_start_pc = sbi_entry.unwrap_or(sbi_addr);
        println!(
            "Linux boot chain: loaded SBI firmware {} start {}",
            sbi_path.display(),
            sbi_start_pc
        );
        sbi_start_pc
    } else {
        let file_entry = load_file(&mut bus, &opts.file, requested_pc)?;
        file_entry.unwrap_or(requested_pc)
    };
    bus.print_memory_map();

    let mut cpu = Cpu::with_pc(bus, start_pc);

    // honor CLI flag to treat ECALL(a7==93) as program exit when requested
    if opts.ecall_exit {
        cpu.set_accept_ecall_exit(true);
    }

    apply_linux_boot_context(&mut cpu, &opts.linux, opts.memory_mb)?;

    if opts.linux.enabled && opts.linux.sbi.is_some() {
        let payload_addr = parse_hex_address(&opts.linux.payload_addr)?;
        inject_opensbi_dynamic_info(&mut cpu, payload_addr, opts.linux.hartid)?;
    }

    println!("\nmyCPU RISC-V Simulator v{}", mycpu::VERSION);
    println!("Starting PC: {}", start_pc);
    if start_pc != requested_pc {
        println!(
            "Requested PC {} overridden by ELF entry {}",
            requested_pc, start_pc
        );
    }
    println!("Memory size: {} MB", opts.memory_mb);
    println!(
        "Max instructions: {}",
        if opts.max_count == 0 {
            "unlimited".to_string()
        } else {
            opts.max_count.to_string()
        }
    );
    println!("\n--- Starting execution ---\n");

    if opts.heartbeat_every > 0 {
        println!(
            "Heartbeat enabled: every {} instructions (mode={:?})",
            opts.heartbeat_every, opts.heartbeat_mode
        );
    }

    let mut uart_injector = opts
        .uart_script
        .map(|script| {
            UartInjector::new(
                parse_escaped_uart_script(&script),
                opts.uart_inject_at,
                opts.uart_inject_every.max(1),
                opts.uart_inject_trigger,
                Arc::clone(&uart_prompt_ready),
            )
        })
        .filter(|injector| !injector.is_empty());

    if let Some(injector) = uart_injector.as_ref() {
        println!(
            "UART script injection enabled: {} bytes, trigger={:?}, start_at={}, every={} steps",
            injector.total_len(),
            injector.trigger,
            injector.inject_at,
            injector.inject_every
        );
    }

    let mut input_injector = opts
        .input_script
        .map(|script| {
            parse_input_script(&script).map(|actions| {
                InputInjector::new(
                    actions,
                    opts.input_inject_at,
                    opts.input_inject_every.max(1),
                )
            })
        })
        .transpose()?
        .filter(|injector| !injector.is_empty());

    if let Some(injector) = input_injector.as_ref() {
        println!(
            "Input script injection enabled: {} actions, start_at={}, every={} steps",
            injector.total_len(),
            injector.inject_at,
            injector.inject_every
        );
    }

    let start_time = Instant::now();
    let instructions_executed =
        if opts.heartbeat_every == 0 && uart_injector.is_none() && input_injector.is_none() {
            cpu.run(opts.max_count)?
        } else {
            run_with_heartbeat(
                &mut cpu,
                opts.max_count,
                opts.heartbeat_every,
                opts.heartbeat_mode,
                uart_injector.as_mut(),
                input_injector.as_mut(),
            )?
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

    if let Some(injector) = uart_injector.as_ref() {
        println!(
            "UART script injected: {}/{} bytes",
            injector.injected_len(),
            injector.total_len()
        );
    }

    if let Some(injector) = input_injector.as_ref() {
        println!(
            "Input script injected: {}/{} actions",
            injector.injected_len(),
            injector.total_len()
        );
    }

    if opts.verbose {
        println!("\nFinal register state:");
        println!("{}", cpu.registers());
    }

    // Generate performance report if requested
    if opts.show_perf_report {
        let report = PerfReport::from_collector(cpu.perf_collector());
        println!("{}", report);
        // If CI requests JSON output, write report to path in MYCPU_PERF_JSON
        if let Ok(path) = env::var("MYCPU_PERF_JSON") {
            match validated_perf_json_path(&path) {
                Ok(valid_path) => match report.write_json_to(&valid_path) {
                    Ok(_) => eprintln!("Perf JSON written to {}", valid_path.display()),
                    Err(e) => eprintln!(
                        "Failed to write perf JSON to {}: {}",
                        valid_path.display(),
                        e
                    ),
                },
                Err(msg) => {
                    eprintln!("Ignoring MYCPU_PERF_JSON='{}': {}", path, msg);
                }
            }
        }
    }

    Ok(())
}

fn validated_perf_json_path(raw: &str) -> std::result::Result<PathBuf, &'static str> {
    let path = Path::new(raw);
    if path.is_absolute() {
        return Err("absolute paths are not allowed");
    }

    if path
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)))
    {
        return Err("path traversal is not allowed");
    }

    if !path.starts_with("artifacts") {
        return Err("path must be under artifacts/");
    }

    Ok(path.to_path_buf())
}

#[derive(Debug, Clone)]
struct UartInjector {
    bytes: Vec<u8>,
    next_index: usize,
    injected_total: usize,
    inject_at: u64,
    inject_every: u64,
    trigger: UartInjectTrigger,
    prompt_ready: Arc<AtomicBool>,
    prompt_chunks: Vec<Vec<u8>>,
    prompt_chunk_index: usize,
    prompt_chunk_offset: usize,
    prompt_wait_for_prompt: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InputAction {
    key_code: u8,
    pressed: bool,
}

#[derive(Debug, Clone)]
struct InputInjector {
    actions: Vec<InputAction>,
    next_index: usize,
    injected_total: usize,
    inject_at: u64,
    inject_every: u64,
}

#[derive(Debug, Default)]
struct PromptDetectorState {
    last_was_dollar: bool,
}

impl PromptDetectorState {
    fn observe_byte(&mut self, byte: u8) -> bool {
        let detected = self.last_was_dollar && byte == b' ';
        self.last_was_dollar = byte == b'$';
        detected
    }
}

impl UartInjector {
    fn new(
        bytes: Vec<u8>,
        inject_at: u64,
        inject_every: u64,
        trigger: UartInjectTrigger,
        prompt_ready: Arc<AtomicBool>,
    ) -> Self {
        let prompt_chunks = split_prompt_chunks(&bytes);
        Self {
            bytes,
            next_index: 0,
            injected_total: 0,
            inject_at,
            inject_every,
            trigger,
            prompt_ready,
            prompt_chunks,
            prompt_chunk_index: 0,
            prompt_chunk_offset: 0,
            prompt_wait_for_prompt: true,
        }
    }

    fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    fn injected_len(&self) -> usize {
        self.injected_total
    }

    fn total_len(&self) -> usize {
        self.bytes.len()
    }

    fn maybe_inject(&mut self, step: u64, cpu: &mut Cpu) {
        match self.trigger {
            UartInjectTrigger::Step => {
                if self.next_index >= self.bytes.len() {
                    return;
                }
                if step < self.inject_at {
                    return;
                }
                if !(step - self.inject_at).is_multiple_of(self.inject_every) {
                    return;
                }

                let byte = self.bytes[self.next_index];
                if cpu.bus_mut().inject_uart_byte(byte) {
                    self.next_index += 1;
                    self.injected_total += 1;
                }
            }
            UartInjectTrigger::Prompt => {
                if self.prompt_chunk_index >= self.prompt_chunks.len() {
                    return;
                }

                if self.prompt_wait_for_prompt {
                    if !self.prompt_ready.swap(false, Ordering::AcqRel) {
                        return;
                    }
                    self.prompt_wait_for_prompt = false;
                }

                if self.inject_every > 1 && !step.is_multiple_of(self.inject_every) {
                    return;
                }

                let byte = self.prompt_chunks[self.prompt_chunk_index][self.prompt_chunk_offset];
                if cpu.bus_mut().inject_uart_byte(byte) {
                    self.prompt_chunk_offset += 1;
                    self.injected_total += 1;
                    if self.prompt_chunk_offset >= self.prompt_chunks[self.prompt_chunk_index].len()
                    {
                        self.prompt_chunk_index += 1;
                        self.prompt_chunk_offset = 0;
                        self.prompt_wait_for_prompt = true;
                    }
                }
            }
        }
    }
}

impl InputInjector {
    fn new(actions: Vec<InputAction>, inject_at: u64, inject_every: u64) -> Self {
        Self {
            actions,
            next_index: 0,
            injected_total: 0,
            inject_at,
            inject_every,
        }
    }

    fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    fn injected_len(&self) -> usize {
        self.injected_total
    }

    fn total_len(&self) -> usize {
        self.actions.len()
    }

    fn maybe_inject(&mut self, step: u64, cpu: &mut Cpu) {
        if self.next_index >= self.actions.len() {
            return;
        }
        if step < self.inject_at {
            return;
        }
        if !(step - self.inject_at).is_multiple_of(self.inject_every) {
            return;
        }

        let action = self.actions[self.next_index];
        if cpu
            .bus_mut()
            .inject_input_key(action.key_code, action.pressed)
        {
            self.next_index += 1;
            self.injected_total += 1;
        }
    }
}

fn split_prompt_chunks(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut chunks = Vec::new();
    let mut start = 0usize;

    for (idx, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            chunks.push(bytes[start..=idx].to_vec());
            start = idx + 1;
        }
    }

    if start < bytes.len() {
        chunks.push(bytes[start..].to_vec());
    }

    chunks
        .into_iter()
        .filter(|chunk| !chunk.is_empty())
        .collect()
}

fn apply_linux_boot_context(
    cpu: &mut Cpu,
    linux: &LinuxBootOptions,
    memory_mb: usize,
) -> anyhow::Result<()> {
    if !linux.enabled {
        return Ok(());
    }

    let dtb_addr = parse_hex_address(&linux.dtb_addr)?;
    let bootargs_addr = parse_hex_address(&linux.bootargs_addr)?;

    let dtb_ptr = if let Some(path) = linux.dtb.as_ref() {
        let dtb = std::fs::read(path)?;
        if dtb.is_empty() {
            return Err(anyhow::anyhow!(
                "Linux DTB file is empty: {}",
                path.display()
            ));
        }
        cpu.bus_mut().write_bytes(dtb_addr, &dtb)?;
        println!(
            "Linux boot: loaded DTB {} ({} bytes) at {}",
            path.display(),
            dtb.len(),
            dtb_addr
        );
        dtb_addr.raw()
    } else if linux.auto_dtb {
        let memory_size_bytes = (memory_mb as u64)
            .saturating_mul(1024)
            .saturating_mul(1024)
            .min(u32::MAX as u64) as u32;
        let dtb = generate_minimal_linux_dtb(memory_size_bytes, linux.bootargs.as_deref());
        cpu.bus_mut().write_bytes(dtb_addr, &dtb)?;
        println!(
            "Linux boot: auto-generated DTB ({} bytes, mem={} MB) at {}",
            dtb.len(),
            memory_mb,
            dtb_addr
        );
        dtb_addr.raw()
    } else {
        0
    };

    if let Some(bootargs) = linux.bootargs.as_ref() {
        let mut bootargs_bytes = bootargs.as_bytes().to_vec();
        bootargs_bytes.push(0);
        cpu.bus_mut().write_bytes(bootargs_addr, &bootargs_bytes)?;
        println!(
            "Linux boot: wrote bootargs ({} bytes incl. NUL) at {}",
            bootargs_bytes.len(),
            bootargs_addr
        );
    }

    cpu.registers_mut()
        .write(RegIdx::new(10), Word::new(linux.hartid));
    cpu.registers_mut()
        .write(RegIdx::new(11), Word::new(dtb_ptr));

    println!(
        "Linux boot context: a0(hartid)={}, a1(dtb)=0x{:08x}",
        linux.hartid, dtb_ptr
    );

    Ok(())
}

fn inject_opensbi_dynamic_info(
    cpu: &mut Cpu,
    payload_addr: Addr,
    linux_hartid: u32,
) -> anyhow::Result<()> {
    let info_addr = Addr::new(OPENSBI_DYNAMIC_INFO_ADDR);
    let words = [
        OPENSBI_DYNAMIC_INFO_MAGIC,
        OPENSBI_DYNAMIC_INFO_VERSION_2,
        payload_addr.raw(),
        OPENSBI_DYNAMIC_INFO_NEXT_MODE_S,
        0,
        linux_hartid,
    ];

    for (index, word) in words.iter().enumerate() {
        cpu.bus_mut()
            .write_word(info_addr.add((index as u32) * 4), Word::new(*word))?;
    }

    // a2 points to fw_dynamic_info for OpenSBI firmware handoff.
    cpu.registers_mut()
        .write(RegIdx::new(12), Word::new(info_addr.raw()));

    println!(
        "Linux boot: wrote OpenSBI fw_dynamic_info at {} (next=0x{:08x}, mode=S, boot_hart={})",
        info_addr,
        payload_addr.raw(),
        linux_hartid
    );

    Ok(())
}

const FDT_MAGIC: u32 = 0xD00D_FEED;
const FDT_BEGIN_NODE: u32 = 0x0000_0001;
const FDT_END_NODE: u32 = 0x0000_0002;
const FDT_PROP: u32 = 0x0000_0003;
const FDT_END: u32 = 0x0000_0009;

fn generate_minimal_linux_dtb(memory_size_bytes: u32, bootargs: Option<&str>) -> Vec<u8> {
    fn push_be32(buf: &mut Vec<u8>, value: u32) {
        buf.extend_from_slice(&value.to_be_bytes());
    }

    fn align4(buf: &mut Vec<u8>) {
        while !buf.len().is_multiple_of(4) {
            buf.push(0);
        }
    }

    fn push_begin_node(buf: &mut Vec<u8>, name: &str) {
        push_be32(buf, FDT_BEGIN_NODE);
        buf.extend_from_slice(name.as_bytes());
        buf.push(0);
        align4(buf);
    }

    fn push_end_node(buf: &mut Vec<u8>) {
        push_be32(buf, FDT_END_NODE);
    }

    fn push_prop(buf: &mut Vec<u8>, nameoff: u32, data: &[u8]) {
        push_be32(buf, FDT_PROP);
        push_be32(buf, data.len() as u32);
        push_be32(buf, nameoff);
        buf.extend_from_slice(data);
        align4(buf);
    }

    let mut strings = Vec::new();
    let mut add_string = |s: &str| -> u32 {
        let off = strings.len() as u32;
        strings.extend_from_slice(s.as_bytes());
        strings.push(0);
        off
    };

    let off_compatible = add_string("compatible");
    let off_model = add_string("model");
    let off_address_cells = add_string("#address-cells");
    let off_size_cells = add_string("#size-cells");
    let off_bootargs = add_string("bootargs");
    let off_device_type = add_string("device_type");
    let off_reg = add_string("reg");

    let mut structure = Vec::new();
    push_begin_node(&mut structure, "");
    push_prop(&mut structure, off_compatible, b"mycpu,virt\0");
    push_prop(&mut structure, off_model, b"mycpu-rv32\0");
    push_prop(&mut structure, off_address_cells, &1u32.to_be_bytes());
    push_prop(&mut structure, off_size_cells, &1u32.to_be_bytes());

    push_begin_node(&mut structure, "chosen");
    if let Some(args) = bootargs {
        let mut data = args.as_bytes().to_vec();
        data.push(0);
        push_prop(&mut structure, off_bootargs, &data);
    }
    push_end_node(&mut structure);

    push_begin_node(&mut structure, "memory@80000000");
    push_prop(&mut structure, off_device_type, b"memory\0");
    let mut reg = Vec::with_capacity(8);
    reg.extend_from_slice(&0x8000_0000u32.to_be_bytes());
    reg.extend_from_slice(&memory_size_bytes.to_be_bytes());
    push_prop(&mut structure, off_reg, &reg);
    push_end_node(&mut structure);

    push_end_node(&mut structure);
    push_be32(&mut structure, FDT_END);
    align4(&mut structure);

    let off_mem_rsvmap = 40u32;
    let mem_rsvmap = vec![0u8; 16];
    let off_dt_struct = off_mem_rsvmap + mem_rsvmap.len() as u32;
    let off_dt_strings = off_dt_struct + structure.len() as u32;
    let totalsize = off_dt_strings + strings.len() as u32;

    let mut out = Vec::with_capacity(totalsize as usize);
    push_be32(&mut out, FDT_MAGIC);
    push_be32(&mut out, totalsize);
    push_be32(&mut out, off_dt_struct);
    push_be32(&mut out, off_dt_strings);
    push_be32(&mut out, off_mem_rsvmap);
    push_be32(&mut out, 17);
    push_be32(&mut out, 16);
    push_be32(&mut out, 0);
    push_be32(&mut out, strings.len() as u32);
    push_be32(&mut out, structure.len() as u32);
    out.extend_from_slice(&mem_rsvmap);
    out.extend_from_slice(&structure);
    out.extend_from_slice(&strings);

    out
}

fn parse_escaped_uart_script(script: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(script.len());
    let mut chars = script.chars();

    while let Some(ch) = chars.next() {
        if ch != '\\' {
            let mut buf = [0u8; 4];
            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            continue;
        }

        match chars.next() {
            Some('n') => out.push(b'\n'),
            Some('r') => out.push(b'\r'),
            Some('t') => out.push(b'\t'),
            Some('0') => out.push(0),
            Some('\\') => out.push(b'\\'),
            Some(other) => {
                out.push(b'\\');
                let mut buf = [0u8; 4];
                out.extend_from_slice(other.encode_utf8(&mut buf).as_bytes());
            }
            None => out.push(b'\\'),
        }
    }

    out
}

fn parse_u32_auto(input: &str) -> Option<u32> {
    if let Some(hex) = input
        .strip_prefix("0x")
        .or_else(|| input.strip_prefix("0X"))
    {
        u32::from_str_radix(hex, 16).ok()
    } else {
        input.parse::<u32>().ok()
    }
}

fn parse_input_key_code(input: &str) -> Option<u8> {
    match input.to_ascii_lowercase().as_str() {
        "up" | "w" => Some(0),
        "left" | "a" => Some(1),
        "down" | "s" => Some(2),
        "right" | "d" => Some(3),
        "btn_a" | "action" | "j" => Some(4),
        "btn_b" | "back" | "k" => Some(5),
        "start" => Some(6),
        "select" => Some(7),
        other => parse_u32_auto(other).and_then(|v| (v <= u8::MAX as u32).then_some(v as u8)),
    }
}

fn parse_input_pressed(input: &str) -> Option<bool> {
    match input.to_ascii_lowercase().as_str() {
        "down" | "press" | "pressed" | "1" => Some(true),
        "up" | "release" | "released" | "0" => Some(false),
        _ => None,
    }
}

fn parse_input_script(script: &str) -> anyhow::Result<Vec<InputAction>> {
    let decoded = String::from_utf8_lossy(&parse_escaped_uart_script(script)).to_string();
    let normalized = decoded.replace(';', "\n");
    let mut actions = Vec::new();

    for raw_line in normalized.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if line.eq_ignore_ascii_case("clear") {
            for key_code in 0u8..8u8 {
                actions.push(InputAction {
                    key_code,
                    pressed: false,
                });
            }
            continue;
        }

        let parsed = if let Some((left, right)) = line.split_once(':') {
            (left.trim(), right.trim())
        } else {
            let mut parts = line.split_whitespace();
            let key = parts.next().ok_or_else(|| {
                anyhow::anyhow!("invalid input script item '{}': missing key", line)
            })?;
            let state = parts.next().ok_or_else(|| {
                anyhow::anyhow!("invalid input script item '{}': missing state", line)
            })?;
            if parts.next().is_some() {
                return Err(anyhow::anyhow!(
                    "invalid input script item '{}': expected '<key> <down|up>'",
                    line
                ));
            }
            (key, state)
        };

        let key_code = parse_input_key_code(parsed.0)
            .ok_or_else(|| anyhow::anyhow!("invalid input key '{}'", parsed.0))?;
        let pressed = parse_input_pressed(parsed.1)
            .ok_or_else(|| anyhow::anyhow!("invalid input state '{}'", parsed.1))?;

        actions.push(InputAction { key_code, pressed });
    }

    Ok(actions)
}

fn read_word_or(cpu: &Cpu, addr: u32, default: u32) -> u32 {
    cpu.bus()
        .read_word(Addr::new(addr))
        .ok()
        .map(|w| w.raw())
        .unwrap_or(default)
}

fn collect_sv32_debug(cpu: &Cpu, satp: u32, sepc_raw: u32) -> (u32, u32, u32) {
    if (satp & 0x8000_0000) == 0 {
        return (0, 0, 0);
    }

    let root = (satp & 0x003F_FFFF) << 12;
    let vpn1 = (sepc_raw >> 22) & 0x3FF;
    let vpn0 = (sepc_raw >> 12) & 0x3FF;
    let pte1_addr = root.wrapping_add(vpn1 * 4);
    let pte1 = read_word_or(cpu, pte1_addr, 0);
    let is_leaf1 = (pte1 & ((1 << 1) | (1 << 3))) != 0;
    let pte0 = if pte1 != 0 && !is_leaf1 {
        let next = ((pte1 >> 10) & 0x003F_FFFF) << 12;
        let pte0_addr = next.wrapping_add(vpn0 * 4);
        read_word_or(cpu, pte0_addr, 0)
    } else {
        0
    };

    (root, pte1, pte0)
}

fn collect_proc_state_summary(cpu: &Cpu) -> (u32, u32, u32, u32, i32, u32) {
    let mut proc_runnable = 0u32;
    let mut proc_running = 0u32;
    let mut proc_sleeping = 0u32;
    let mut proc_runnable_locked = 0u32;
    let mut first_runnable_idx: i32 = -1;
    let mut first_runnable_lock_cpu = 0u32;

    for i in 0..XV6_NPROC {
        let proc_base = XV6_PROC_BASE + i * XV6_PROC_STRIDE;
        let state_addr = proc_base + PROC_STATE_OFFSET;
        if let Ok(state) = cpu.bus().read_word(Addr::new(state_addr)) {
            match state.raw() {
                1 => proc_sleeping += 1,
                2 => {
                    proc_runnable += 1;
                    if first_runnable_idx < 0 {
                        first_runnable_idx = i as i32;
                    }
                    let lock_word = read_word_or(cpu, proc_base, 0);
                    if lock_word != 0 {
                        proc_runnable_locked += 1;
                    }
                    if first_runnable_idx == i as i32 {
                        first_runnable_lock_cpu = read_word_or(cpu, proc_base + 8, 0);
                    }
                }
                3 => proc_running += 1,
                _ => {}
            }
        }
    }

    (
        proc_runnable,
        proc_running,
        proc_sleeping,
        proc_runnable_locked,
        first_runnable_idx,
        first_runnable_lock_cpu,
    )
}

#[derive(Debug, Clone, Copy, Default)]
struct VirtioActivity {
    cmds: u64,
    notifies: u64,
    desc: u64,
    irq_raised: u64,
    irq_ack: u64,
    desc_not_ready: u64,
    desc_no_avail: u64,
    desc_success: u64,
    desc_error: u64,
}

#[derive(Debug, Clone)]
struct HeartbeatCommonMetrics {
    count: u64,
    pc: Addr,
    privilege: String,
    tp: u32,
    global_mie: bool,
    global_sie: bool,
    mip: u32,
    mie: u32,
    sip: u32,
    sie: u32,
    satp: u32,
    scause: u32,
    sepc: u32,
    stval: u32,
    sv32_root: u32,
    sv32_pte1: u32,
    sv32_pte0: u32,
    mtip: bool,
    msip: bool,
    meip: bool,
    seip: bool,
    virtio_irq: bool,
    uart_irq: bool,
    virtio: VirtioActivity,
    same_pc_streak: u64,
}

#[derive(Debug, Clone, Copy)]
struct HeartbeatDiagnosticMetrics {
    cpu0_proc: u32,
    cpu0_noff: i32,
    cpu0_intena: u32,
    ticks: u32,
    clint_mtime: u32,
    clint_mtimecmp: u32,
    mscratch: u32,
    scratch_interval: u32,
    tickslock_locked: u32,
    tickslock_cpu: u32,
    proc0_state: u32,
    proc0_pid: u32,
    proc0_ctx_ra: u32,
    proc0_state_cpu_view: u32,
    plic_pending0: u32,
    plic_senable0: u32,
    plic_sthreshold0: u32,
    plic_sclaim_peek: u32,
    proc_runnable: u32,
    proc_running: u32,
    proc_sleeping: u32,
    proc_runnable_locked: u32,
    first_runnable_idx: i32,
    first_runnable_lock_cpu: u32,
}

fn as_bit(value: bool) -> u8 {
    if value {
        1
    } else {
        0
    }
}

fn flush_stdout_silent() {
    std::io::stdout().flush().ok();
}

type HeartbeatFieldList = Vec<(&'static str, String)>;

fn push_field<T: ToString>(fields: &mut HeartbeatFieldList, key: &'static str, value: T) {
    fields.push((key, value.to_string()));
}

fn push_hex_u32(fields: &mut HeartbeatFieldList, key: &'static str, value: u32) {
    fields.push((key, format!("0x{value:08x}")));
}

fn render_heartbeat_line(tag: &str, fields: &HeartbeatFieldList) -> String {
    let payload = fields
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(" ");
    format!("[{tag}] {payload}")
}

fn build_compact_heartbeat_fields(common: &HeartbeatCommonMetrics) -> HeartbeatFieldList {
    let mut fields = HeartbeatFieldList::with_capacity(26);
    push_field(&mut fields, "step", common.count);
    push_field(&mut fields, "pc", common.pc);
    push_field(&mut fields, "priv", &common.privilege);
    push_hex_u32(&mut fields, "tp", common.tp);
    push_field(&mut fields, "mstatus.mie", as_bit(common.global_mie));
    push_field(&mut fields, "sstatus.sie", as_bit(common.global_sie));
    push_hex_u32(&mut fields, "mip", common.mip);
    push_hex_u32(&mut fields, "mie", common.mie);
    push_hex_u32(&mut fields, "sip", common.sip);
    push_hex_u32(&mut fields, "sie", common.sie);
    push_hex_u32(&mut fields, "satp", common.satp);
    push_hex_u32(&mut fields, "scause", common.scause);
    push_hex_u32(&mut fields, "sepc", common.sepc);
    push_hex_u32(&mut fields, "stval", common.stval);
    push_hex_u32(&mut fields, "sv32.root", common.sv32_root);
    push_hex_u32(&mut fields, "sv32.pte1", common.sv32_pte1);
    push_hex_u32(&mut fields, "sv32.pte0", common.sv32_pte0);
    push_field(&mut fields, "mtip", as_bit(common.mtip));
    push_field(&mut fields, "msip", as_bit(common.msip));
    push_field(&mut fields, "meip", as_bit(common.meip));
    push_field(&mut fields, "seip", as_bit(common.seip));
    push_field(&mut fields, "virtio_irq", as_bit(common.virtio_irq));
    push_field(&mut fields, "uart_irq", as_bit(common.uart_irq));
    push_field(&mut fields, "v_notify", common.virtio.notifies);
    push_field(&mut fields, "v_desc_ok", common.virtio.desc_success);
    push_field(&mut fields, "pc_streak", common.same_pc_streak);
    fields
}

fn build_diagnostic_heartbeat_fields(
    common: &HeartbeatCommonMetrics,
    diag: &HeartbeatDiagnosticMetrics,
) -> HeartbeatFieldList {
    let mut fields = HeartbeatFieldList::with_capacity(57);
    push_field(&mut fields, "step", common.count);
    push_field(&mut fields, "pc", common.pc);
    push_field(&mut fields, "priv", &common.privilege);
    push_hex_u32(&mut fields, "tp", common.tp);
    push_field(&mut fields, "mstatus.mie", as_bit(common.global_mie));
    push_field(&mut fields, "sstatus.sie", as_bit(common.global_sie));
    push_hex_u32(&mut fields, "mip", common.mip);
    push_hex_u32(&mut fields, "mie", common.mie);
    push_hex_u32(&mut fields, "sip", common.sip);
    push_hex_u32(&mut fields, "sie", common.sie);
    push_hex_u32(&mut fields, "satp", common.satp);
    push_hex_u32(&mut fields, "scause", common.scause);
    push_hex_u32(&mut fields, "sepc", common.sepc);
    push_hex_u32(&mut fields, "stval", common.stval);
    push_hex_u32(&mut fields, "sv32.root", common.sv32_root);
    push_hex_u32(&mut fields, "sv32.pte1", common.sv32_pte1);
    push_hex_u32(&mut fields, "sv32.pte0", common.sv32_pte0);
    push_field(&mut fields, "mtip", as_bit(common.mtip));
    push_field(&mut fields, "msip", as_bit(common.msip));
    push_field(&mut fields, "meip", as_bit(common.meip));
    push_field(&mut fields, "virtio_irq", as_bit(common.virtio_irq));
    push_field(&mut fields, "uart_irq", as_bit(common.uart_irq));
    push_field(&mut fields, "v_cmd", common.virtio.cmds);
    push_field(&mut fields, "v_notify", common.virtio.notifies);
    push_field(&mut fields, "v_desc", common.virtio.desc);
    push_field(&mut fields, "v_irq_raise", common.virtio.irq_raised);
    push_field(&mut fields, "v_irq_ack", common.virtio.irq_ack);
    push_field(
        &mut fields,
        "v_desc_not_ready",
        common.virtio.desc_not_ready,
    );
    push_field(&mut fields, "v_desc_no_avail", common.virtio.desc_no_avail);
    push_field(&mut fields, "v_desc_ok", common.virtio.desc_success);
    push_field(&mut fields, "v_desc_err", common.virtio.desc_error);
    push_hex_u32(&mut fields, "plic_pending0", diag.plic_pending0);
    push_hex_u32(&mut fields, "plic_senable0", diag.plic_senable0);
    push_hex_u32(&mut fields, "plic_sth", diag.plic_sthreshold0);
    push_field(&mut fields, "plic_sclaim", diag.plic_sclaim_peek);
    push_hex_u32(&mut fields, "cpu0_proc", diag.cpu0_proc);
    push_field(&mut fields, "cpu0_noff", diag.cpu0_noff);
    push_field(&mut fields, "cpu0_intena", diag.cpu0_intena);
    push_field(&mut fields, "ticks", diag.ticks);
    push_field(&mut fields, "mtime", diag.clint_mtime);
    push_field(&mut fields, "mtimecmp", diag.clint_mtimecmp);
    push_hex_u32(&mut fields, "mscratch", diag.mscratch);
    push_field(&mut fields, "scratch5", diag.scratch_interval);
    push_field(&mut fields, "tickslock_locked", diag.tickslock_locked);
    push_hex_u32(&mut fields, "tickslock_cpu", diag.tickslock_cpu);
    push_field(&mut fields, "p0_state", diag.proc0_state);
    push_field(&mut fields, "p0_state_cpu", diag.proc0_state_cpu_view);
    push_field(&mut fields, "p0_pid", diag.proc0_pid);
    push_hex_u32(&mut fields, "p0_ctx_ra", diag.proc0_ctx_ra);
    push_field(&mut fields, "p_run", diag.proc_runnable);
    push_field(&mut fields, "p_run_locked", diag.proc_runnable_locked);
    push_field(&mut fields, "p_run0_idx", diag.first_runnable_idx);
    push_hex_u32(&mut fields, "p_run0_lock_cpu", diag.first_runnable_lock_cpu);
    push_field(&mut fields, "p_running", diag.proc_running);
    push_field(&mut fields, "p_sleep", diag.proc_sleeping);
    push_field(&mut fields, "pc_streak", common.same_pc_streak);
    fields
}

fn collect_virtio_activity(cpu: &Cpu) -> VirtioActivity {
    let (
        cmds,
        notifies,
        desc,
        irq_raised,
        irq_ack,
        desc_not_ready,
        desc_no_avail,
        desc_success,
        desc_error,
    ) = cpu
        .bus()
        .get_virtio_activity_counters()
        .unwrap_or((0, 0, 0, 0, 0, 0, 0, 0, 0));

    VirtioActivity {
        cmds,
        notifies,
        desc,
        irq_raised,
        irq_ack,
        desc_not_ready,
        desc_no_avail,
        desc_success,
        desc_error,
    }
}

fn collect_heartbeat_common(cpu: &Cpu, count: u64, same_pc_streak: u64) -> HeartbeatCommonMetrics {
    let pc = cpu.pc();
    let tp = cpu.registers().read(RegIdx::new(4)).raw();
    let csr = cpu.csr();
    let mip = csr.mip.read();
    let mie = csr.mie.read();
    let global_mie = csr.mstatus.mie();
    let sip = csr.sip.read();
    let sie = csr.sie.read();
    let scause = csr.scause.read();
    let sepc = csr.sepc.read();
    let stval = csr.stval.read();
    let satp = csr.satp.read();
    let global_sie = csr.sstatus.sie();
    let (sv32_root, sv32_pte1, sv32_pte0) = collect_sv32_debug(cpu, satp, sepc);
    let (mtip, msip) = cpu.bus().get_clint_interrupt_status();
    let (meip, seip) = cpu.bus().get_plic_interrupt_status();
    let virtio_irq = cpu.bus().has_peripheral_interrupt("VirtIO-Block");
    let uart_irq = cpu.bus().has_peripheral_interrupt("UART");
    let virtio = collect_virtio_activity(cpu);

    HeartbeatCommonMetrics {
        count,
        pc,
        privilege: cpu.privilege().to_string(),
        tp,
        global_mie,
        global_sie,
        mip,
        mie,
        sip,
        sie,
        satp,
        scause,
        sepc,
        stval,
        sv32_root,
        sv32_pte1,
        sv32_pte0,
        mtip,
        msip,
        meip,
        seip,
        virtio_irq,
        uart_irq,
        virtio,
        same_pc_streak,
    }
}

fn collect_heartbeat_diagnostic(cpu: &mut Cpu) -> HeartbeatDiagnosticMetrics {
    let cpu0_proc = read_word_or(cpu, XV6_CPU0_ADDR + CPU_PROC_OFFSET, 0);
    let cpu0_noff = read_word_or(cpu, XV6_CPU0_ADDR + CPU_NOFF_OFFSET, u32::MAX) as i32;
    let cpu0_intena = read_word_or(cpu, XV6_CPU0_ADDR + CPU_INTENA_OFFSET, 0);
    let ticks = read_word_or(cpu, XV6_TICKS_ADDR, 0);
    let clint_mtime = read_word_or(cpu, CLINT_MTIME_ADDR, 0);
    let clint_mtimecmp = read_word_or(cpu, CLINT_MTIMECMP_ADDR, 0);
    let mscratch = cpu.csr().mscratch.get();
    let scratch_interval = read_word_or(cpu, XV6_MSCRATCH0_ADDR + 20, 0);
    let tickslock_locked = read_word_or(cpu, XV6_TICKSLOCK_ADDR, 0);
    let tickslock_cpu = read_word_or(cpu, XV6_TICKSLOCK_ADDR + 8, 0);
    let proc0_state = read_word_or(cpu, XV6_PROC_BASE + PROC_STATE_OFFSET, 0);
    let proc0_pid = read_word_or(cpu, XV6_PROC_BASE + 32, 0);
    let proc0_ctx_ra = read_word_or(cpu, XV6_PROC_BASE + 52, 0);
    let proc0_state_cpu_view = cpu
        .read_word(Addr::new(XV6_PROC_BASE + PROC_STATE_OFFSET))
        .ok()
        .map(|w| w.raw())
        .unwrap_or(u32::MAX);
    let plic_pending0 = read_word_or(cpu, PLIC_PENDING0_ADDR, 0);
    let plic_senable0 = read_word_or(cpu, PLIC_SENABLE0_ADDR, 0);
    let plic_sthreshold0 = read_word_or(cpu, PLIC_STHRESHOLD0_ADDR, 0);
    let plic_sclaim_peek = read_word_or(cpu, PLIC_SCLAIM_PEEK_ADDR, 0);
    let (
        proc_runnable,
        proc_running,
        proc_sleeping,
        proc_runnable_locked,
        first_runnable_idx,
        first_runnable_lock_cpu,
    ) = collect_proc_state_summary(cpu);

    HeartbeatDiagnosticMetrics {
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
        proc0_pid,
        proc0_ctx_ra,
        proc0_state_cpu_view,
        plic_pending0,
        plic_senable0,
        plic_sthreshold0,
        plic_sclaim_peek,
        proc_runnable,
        proc_running,
        proc_sleeping,
        proc_runnable_locked,
        first_runnable_idx,
        first_runnable_lock_cpu,
    }
}

fn emit_compact_heartbeat(common: &HeartbeatCommonMetrics) {
    let line = render_heartbeat_line("hb-lite", &build_compact_heartbeat_fields(common));
    println!("{line}");
    flush_stdout_silent();
}

fn emit_diagnostic_heartbeat(common: &HeartbeatCommonMetrics, diag: &HeartbeatDiagnosticMetrics) {
    let line = render_heartbeat_line("hb", &build_diagnostic_heartbeat_fields(common, diag));
    println!("{line}");
    flush_stdout_silent();
}

fn run_with_heartbeat(
    cpu: &mut Cpu,
    max_count: u64,
    heartbeat_every: u64,
    heartbeat_mode: HeartbeatMode,
    mut uart_injector: Option<&mut UartInjector>,
    mut input_injector: Option<&mut InputInjector>,
) -> anyhow::Result<u64> {
    let mut count = 0u64;
    let mut last_hb_pc: Option<Addr> = None;
    let mut same_pc_streak = 0u64;

    while !cpu.is_halted() {
        if max_count > 0 && count >= max_count {
            break;
        }

        cpu.step()?;
        count += 1;

        if let Some(injector) = uart_injector.as_deref_mut() {
            injector.maybe_inject(count, cpu);
        }

        if let Some(injector) = input_injector.as_deref_mut() {
            injector.maybe_inject(count, cpu);
        }

        if heartbeat_every > 0 && count.is_multiple_of(heartbeat_every) {
            let pc = cpu.pc();
            if Some(pc) == last_hb_pc {
                same_pc_streak += 1;
            } else {
                same_pc_streak = 0;
                last_hb_pc = Some(pc);
            }

            let common = collect_heartbeat_common(cpu, count, same_pc_streak);

            if matches!(heartbeat_mode, HeartbeatMode::Compact) {
                emit_compact_heartbeat(&common);
                continue;
            }

            let diag = collect_heartbeat_diagnostic(cpu);
            emit_diagnostic_heartbeat(&common, &diag);
        }
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::{
        generate_minimal_linux_dtb, parse_escaped_uart_script, parse_input_script,
        split_prompt_chunks, InputAction, PromptDetectorState,
    };

    #[test]
    fn test_parse_escaped_uart_script_common_sequences() {
        let parsed = parse_escaped_uart_script("echo hi\\nnext\\tcol\\r");
        assert_eq!(parsed, b"echo hi\nnext\tcol\r");
    }

    #[test]
    fn test_parse_escaped_uart_script_preserves_unknown_escape() {
        let parsed = parse_escaped_uart_script("a\\xb");
        assert_eq!(parsed, b"a\\xb");
    }

    #[test]
    fn test_prompt_detector_state_detects_shell_prompt_sequence() {
        let mut detector = PromptDetectorState::default();
        assert!(!detector.observe_byte(b'x'));
        assert!(!detector.observe_byte(b'$'));
        assert!(detector.observe_byte(b' '));
    }

    #[test]
    fn test_prompt_detector_state_ignores_non_prompt_sequence() {
        let mut detector = PromptDetectorState::default();
        assert!(!detector.observe_byte(b'$'));
        assert!(!detector.observe_byte(b'\n'));
        assert!(!detector.observe_byte(b' '));
    }

    #[test]
    fn test_split_prompt_chunks_by_newline() {
        let chunks = split_prompt_chunks(b"ls\necho OK\n");
        assert_eq!(chunks, vec![b"ls\n".to_vec(), b"echo OK\n".to_vec()]);
    }

    #[test]
    fn test_split_prompt_chunks_keeps_tail_without_newline() {
        let chunks = split_prompt_chunks(b"echo tail");
        assert_eq!(chunks, vec![b"echo tail".to_vec()]);
    }

    #[test]
    fn test_generate_minimal_linux_dtb_has_magic() {
        let dtb = generate_minimal_linux_dtb(128 * 1024 * 1024, None);
        assert!(dtb.len() >= 4);
        assert_eq!(&dtb[0..4], &0xD00D_FEEDu32.to_be_bytes());
    }

    #[test]
    fn test_generate_minimal_linux_dtb_contains_bootargs() {
        let dtb = generate_minimal_linux_dtb(128 * 1024 * 1024, Some("console=ttyS0"));
        let text = String::from_utf8_lossy(&dtb);
        assert!(text.contains("bootargs"));
        assert!(text.contains("console=ttyS0"));
    }

    #[test]
    fn test_parse_input_script_supports_semicolon_and_colon() {
        let actions = parse_input_script("right:down;right:up;btn_a:down").unwrap();
        assert_eq!(
            actions,
            vec![
                InputAction {
                    key_code: 3,
                    pressed: true,
                },
                InputAction {
                    key_code: 3,
                    pressed: false,
                },
                InputAction {
                    key_code: 4,
                    pressed: true,
                },
            ]
        );
    }

    #[test]
    fn test_parse_input_script_supports_escaped_newlines_and_clear() {
        let actions = parse_input_script("up down\\nclear\\nup up").unwrap();
        assert_eq!(
            actions.first(),
            Some(&InputAction {
                key_code: 0,
                pressed: true,
            })
        );
        assert_eq!(actions.len(), 10);
        assert_eq!(
            actions.last(),
            Some(&InputAction {
                key_code: 0,
                pressed: false,
            })
        );
    }

    #[test]
    fn test_parse_input_script_rejects_invalid_item() {
        let err = parse_input_script("unknown down").unwrap_err().to_string();
        assert!(err.contains("invalid input key"));
    }
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
    attach_stdout_uart(&mut bus, None);

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
    bus.attach_peripheral(Gpu::new());
    bus.attach_peripheral(Tpu::new());

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
    bus.attach_peripheral(InputDevice::new());
    Ok(bus)
}

/// Attach UART that outputs to stdout
fn attach_stdout_uart(bus: &mut Bus, prompt_ready: Option<Arc<AtomicBool>>) {
    let mut uart = Uart::with_base(Addr::new(0x1000_0000));
    let prompt_state = Arc::new(Mutex::new(PromptDetectorState::default()));
    let prompt_state_for_cb = Arc::clone(&prompt_state);
    uart.set_output_callback(Box::new(move |byte| {
        print!("{}", byte as char);
        if let Some(flag) = prompt_ready.as_ref() {
            let detected = if let Ok(mut state) = prompt_state_for_cb.lock() {
                state.observe_byte(byte)
            } else {
                false
            };
            if detected {
                flag.store(true, Ordering::Release);
            }
        }
        flush_stdout_silent();
    }));
    bus.attach_peripheral(uart);
}

/// Load a binary or ELF file into memory
fn load_file(bus: &mut Bus, path: &PathBuf, load_addr: Addr) -> anyhow::Result<Option<Addr>> {
    let data = std::fs::read(path)?;

    // Check if it's an ELF file (magic: 0x7F 'E' 'L' 'F')
    if data.len() >= 4 && data[0..4] == [0x7F, b'E', b'L', b'F'] {
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

/// Load a file as raw bytes at the specified address.
fn load_raw_file_at(bus: &mut Bus, path: &PathBuf, load_addr: Addr) -> anyhow::Result<usize> {
    let data = std::fs::read(path)?;
    if data.len() >= 4 && data[0..4] == [0x7F, b'E', b'L', b'F'] {
        println!(
            "Linux boot chain: payload {} looks like ELF, but loaded as raw image at {}",
            path.display(),
            load_addr
        );
    }
    bus.write_bytes(load_addr, &data)?;
    Ok(data.len())
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
    attach_stdout_uart(&mut bus, None);

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
        start_visualize_server_with_initial_pc(cpu, port, start_pc.raw())
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    })
}
