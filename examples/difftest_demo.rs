use mycpu::cpu::Cpu;
use mycpu::difftest::DiffTest;
use mycpu::error::Result;
use mycpu::interrupt::{Clint, Plic};
use mycpu::loader::ElfLoader;
use mycpu::memory::{Bus, Ram};
use mycpu::peripheral::{Gpu, InputDevice, Lpu, Npu, Tpu, Uart, VirtioBlock};
use mycpu::types::Addr;
use std::env;

fn create_bus() -> Result<Bus> {
    let mut bus = Bus::new();
    let memory_size = 128 * 1024 * 1024; // 128 MB
    let ram = Ram::new(memory_size);
    bus.attach_memory(Addr::new(0x8000_0000), ram, "Main RAM");

    bus.attach_peripheral(Clint::new());
    bus.attach_peripheral(Plic::new());

    // Attach common peripherals so programs depending on them behave similarly to QEMU
    bus.attach_peripheral(Npu::new());
    bus.attach_peripheral(Lpu::new());
    bus.attach_peripheral(Gpu::new());
    bus.attach_peripheral(Tpu::new());

    bus.attach_peripheral(VirtioBlock::new());
    bus.attach_peripheral(InputDevice::new());

    // Attach a UART that prints to stdout (useful for demo programs)
    let mut uart = Uart::new();
    uart.set_output_callback(Box::new(|b| print!("{}", b as char)));
    bus.attach_peripheral(uart);

    Ok(bus)
}

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let elf_path = args
        .next()
        .unwrap_or_else(|| "tests/programs/test.elf".to_string());

    println!("DiffTest demo: ELF={}\n", elf_path);

    // Prepare system bus and load program
    let mut bus = create_bus()?;
    let loader = ElfLoader::from_file(&elf_path)?;
    let entry = loader.entry_point();
    println!("Loading ELF into simulated RAM, entry={}", entry);
    loader.load_into(&mut bus)?;

    // Create CPU with entry PC
    let mut cpu = Cpu::with_pc(bus, entry);

    // Connect to QEMU GDB server (assumes QEMU started with -S -s on port 1234)
    let mut difftest = DiffTest::connect("127.0.0.1:1234")?;
    difftest.set_verbose(true);

    println!("Connected to QEMU. Starting step-by-step comparison...\n");

    for i in 0..10_000_000u64 {
        // Step myCPU
        let state = match cpu.step() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("myCPU error during step: {}", e);
                break;
            }
        };

        // Step QEMU and compare
        if let Err(mismatch) = difftest.step_and_compare(&state) {
            eprintln!("\n=== DiffTest MISMATCH ===\n{}", mismatch);
            break;
        }

        if i % 1000 == 0 {
            println!("Compared {} instructions (PC=0x{:08x})", i, state.pc.raw());
        }
    }

    println!("DiffTest demo finished.");
    Ok(())
}
