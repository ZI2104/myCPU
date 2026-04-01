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
use mycpu::perf_report::PerfReport;
use mycpu::peripheral::{Lpu, Npu, Uart, VirtioBlock};
use mycpu::types::{Addr, RegIdx, Word};
use mycpu::visualize::linux_fb_program;
use mycpu::visualize::start_visualize_server;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
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
        } => run_program(
            memory,
            &pc,
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
            linux_boot,
            linux_hartid,
            linux_dtb,
            linux_auto_dtb,
            &linux_dtb_addr,
            linux_bootargs,
            &linux_bootargs_addr,
            linux_sbi,
            &linux_sbi_addr,
            &linux_payload_addr,
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
    heartbeat_mode: HeartbeatMode,
    verbose: bool,
    show_perf_report: bool,
    file: PathBuf,
    virtio_disk: Option<PathBuf>,
    uart_script: Option<String>,
    uart_inject_at: u64,
    uart_inject_every: u64,
    uart_inject_trigger: UartInjectTrigger,
    linux_boot: bool,
    linux_hartid: u32,
    linux_dtb: Option<PathBuf>,
    linux_auto_dtb: bool,
    linux_dtb_addr_str: &str,
    linux_bootargs: Option<String>,
    linux_bootargs_addr_str: &str,
    linux_sbi: Option<PathBuf>,
    linux_sbi_addr_str: &str,
    linux_payload_addr_str: &str,
) -> anyhow::Result<()> {
    init_logger(verbose);

    let requested_pc = parse_hex_address(pc_str)?;
    let mut bus = create_bus(memory_mb, virtio_disk.as_ref())?;
    let uart_prompt_ready = Arc::new(AtomicBool::new(false));

    // Attach UART for output
    attach_stdout_uart(&mut bus, Some(Arc::clone(&uart_prompt_ready)));

    let start_pc = if let Some(sbi_path) = linux_sbi.as_ref() {
        if !linux_boot {
            return Err(anyhow::anyhow!(
                "--linux-sbi requires --linux-boot to be enabled"
            ));
        }

        let sbi_addr = parse_hex_address(linux_sbi_addr_str)?;
        let payload_addr = parse_hex_address(linux_payload_addr_str)?;
        let payload_size = load_raw_file_at(&mut bus, &file, payload_addr)?;
        println!(
            "Linux boot chain: loaded payload {} ({} bytes) at {}",
            file.display(),
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
        let file_entry = load_file(&mut bus, &file, requested_pc)?;
        file_entry.unwrap_or(requested_pc)
    };
    bus.print_memory_map();

    let mut cpu = Cpu::with_pc(bus, start_pc);

    apply_linux_boot_context(
        &mut cpu,
        linux_boot,
        linux_hartid,
        linux_dtb,
        linux_auto_dtb,
        linux_dtb_addr_str,
        linux_bootargs,
        linux_bootargs_addr_str,
        memory_mb,
    )?;

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
        println!(
            "Heartbeat enabled: every {} instructions (mode={:?})",
            heartbeat_every, heartbeat_mode
        );
    }

    let mut uart_injector = uart_script
        .map(|script| {
            UartInjector::new(
                parse_escaped_uart_script(&script),
                uart_inject_at,
                uart_inject_every.max(1),
                uart_inject_trigger,
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

    let start_time = Instant::now();
    let instructions_executed = if heartbeat_every == 0 && uart_injector.is_none() {
        cpu.run(max_count)?
    } else {
        run_with_heartbeat(
            &mut cpu,
            max_count,
            heartbeat_every,
            heartbeat_mode,
            uart_injector.as_mut(),
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
                if (step - self.inject_at) % self.inject_every != 0 {
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

                if self.inject_every > 1 && step % self.inject_every != 0 {
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
    linux_boot: bool,
    linux_hartid: u32,
    linux_dtb: Option<PathBuf>,
    linux_auto_dtb: bool,
    linux_dtb_addr_str: &str,
    linux_bootargs: Option<String>,
    linux_bootargs_addr_str: &str,
    memory_mb: usize,
) -> anyhow::Result<()> {
    if !linux_boot {
        return Ok(());
    }

    let dtb_addr = parse_hex_address(linux_dtb_addr_str)?;
    let bootargs_addr = parse_hex_address(linux_bootargs_addr_str)?;

    let dtb_ptr = if let Some(path) = linux_dtb {
        let dtb = std::fs::read(&path)?;
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
    } else if linux_auto_dtb {
        let memory_size_bytes = (memory_mb as u64)
            .saturating_mul(1024)
            .saturating_mul(1024)
            .min(u32::MAX as u64) as u32;
        let dtb = generate_minimal_linux_dtb(memory_size_bytes, linux_bootargs.as_deref());
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

    if let Some(bootargs) = linux_bootargs {
        let mut bootargs_bytes = bootargs.into_bytes();
        bootargs_bytes.push(0);
        cpu.bus_mut().write_bytes(bootargs_addr, &bootargs_bytes)?;
        println!(
            "Linux boot: wrote bootargs ({} bytes incl. NUL) at {}",
            bootargs_bytes.len(),
            bootargs_addr
        );
    }

    cpu.registers_mut()
        .write(RegIdx::new(10), Word::new(linux_hartid));
    cpu.registers_mut()
        .write(RegIdx::new(11), Word::new(dtb_ptr));

    println!(
        "Linux boot context: a0(hartid)={}, a1(dtb)=0x{:08x}",
        linux_hartid, dtb_ptr
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

fn run_with_heartbeat(
    cpu: &mut Cpu,
    max_count: u64,
    heartbeat_every: u64,
    heartbeat_mode: HeartbeatMode,
    mut uart_injector: Option<&mut UartInjector>,
) -> anyhow::Result<u64> {
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

        if let Some(injector) = uart_injector.as_deref_mut() {
            injector.maybe_inject(count, cpu);
        }

        if heartbeat_every > 0 && count % heartbeat_every == 0 {
            let pc = cpu.pc();
            if Some(pc) == last_hb_pc {
                same_pc_streak += 1;
            } else {
                same_pc_streak = 0;
                last_hb_pc = Some(pc);
            }

            let tp = cpu.registers().read(RegIdx::new(4)).raw();
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
            let (meip, seip) = cpu.bus().get_plic_interrupt_status();
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

            if matches!(heartbeat_mode, HeartbeatMode::Compact) {
                println!(
                    "[hb-lite] step={} pc={} priv={} tp=0x{:08x} mstatus.mie={} sstatus.sie={} mip=0x{:08x} mie=0x{:08x} sip=0x{:08x} sie=0x{:08x} scause=0x{:08x} sepc=0x{:08x} mtip={} msip={} meip={} seip={} virtio_irq={} uart_irq={} v_notify={} v_desc_ok={} pc_streak={}",
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
                    if seip { 1 } else { 0 },
                    if virtio_irq { 1 } else { 0 },
                    if uart_irq { 1 } else { 0 },
                    virtio_notifies,
                    virtio_desc_success,
                    same_pc_streak
                );
                std::io::stdout().flush().ok();
                continue;
            }

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

#[cfg(test)]
mod tests {
    use super::{
        generate_minimal_linux_dtb, parse_escaped_uart_script, split_prompt_chunks,
        PromptDetectorState,
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

/// Load a file as raw bytes at the specified address.
fn load_raw_file_at(bus: &mut Bus, path: &PathBuf, load_addr: Addr) -> anyhow::Result<usize> {
    let data = std::fs::read(path)?;
    if data.len() >= 4 && &data[0..4] == &[0x7F, b'E', b'L', b'F'] {
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
        start_visualize_server(cpu, port)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    })
}
