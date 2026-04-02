use std::path::Path;

use mycpu::cpu::Cpu;
use mycpu::interrupt::{Clint, Plic};
use mycpu::loader::ElfLoader;
use mycpu::memory::{Bus, Ram};
use mycpu::peripheral::{Lpu, Npu};
use mycpu::types::Addr;

#[test]
fn test_npu_elf_end_to_end() {
    let elf_path = Path::new("tests/programs/npu_vector_example.elf");
    if !elf_path.exists() {
        eprintln!("Skipping test_npu_elf_end_to_end: ELF not built (tests/programs/npu_vector_example.elf)");
        return;
    }

    let loader = match ElfLoader::from_file(elf_path) {
        Ok(l) => l,
        Err(e) => panic!("Failed to parse ELF: {}", e),
    };

    // Create bus and attach RAM + peripherals (mirrors create_bus)
    let mut bus = Bus::new();
    bus.attach_memory(
        Addr::new(0x8000_0000),
        Ram::new(16 * 1024 * 1024),
        "Main RAM",
    );
    bus.attach_peripheral(Clint::new());
    bus.attach_peripheral(Plic::new());
    bus.attach_peripheral(Npu::new());
    bus.attach_peripheral(Lpu::new());

    // Load ELF into bus
    loader
        .load_into(&mut bus)
        .expect("Failed to load ELF into bus");
    let entry = loader.entry_point();

    // Debug flag: set environment variable MYCPU_NPU_DEBUG=1 to enable
    let debug = std::env::var("MYCPU_NPU_DEBUG").is_ok();
    macro_rules! dprintln {
        ($($arg:tt)*) => {
            if debug { eprintln!($($arg)*); }
        }
    }

    dprintln!("Loaded ELF entry: {}", entry);
    // Dump first instruction words at entry to verify code was loaded
    match bus.read_word(entry) {
        Ok(w) => dprintln!("Code at entry: 0x{:08x}", w.raw()),
        Err(e) => dprintln!("Failed to read code at entry: {:?}", e),
    }

    // Try to decode first few instructions for debugging
    {
        use mycpu::instruction::Decoder;
        use mycpu::types::Addr;

        let mut p = entry;
        for i in 0..8 {
            match bus.read_half(p) {
                Ok(h) => {
                    let low = h.raw() as u32;
                    if (low & 0b11) != 0b11 {
                        dprintln!("{}: 16-bit instr 0x{:04x} at {}", i, low & 0xFFFF, p);
                        p = Addr::new(p.raw().wrapping_add(2));
                        continue;
                    }
                }
                Err(_) => {
                    dprintln!("Failed to read half at {}", p);
                    break;
                }
            }

            match bus.read_word(p) {
                Ok(w) => match Decoder::decode(w.raw(), p) {
                    Ok(d) => dprintln!("{}: {:#?} @ {} (raw=0x{:08x})", i, d, p, w.raw()),
                    Err(e) => dprintln!("{}: decode error @ {}: {:?}", i, p, e),
                },
                Err(e) => {
                    dprintln!("Failed to read word at {}: {:?}", p, e);
                    break;
                }
            }

            p = Addr::new(p.raw().wrapping_add(4));
        }
    }

    let mut cpu = Cpu::with_pc(bus, entry);
    dprintln!("Initial PC: 0x{:08x}", cpu.pc().raw());

    // Step CPU until NPU has processed something or until instruction limit
    let mut done = false;
    for i in 0..200_000u64 {
        match cpu.step() {
            Ok(_) => {}
            Err(e) => {
                dprintln!("cpu.step() returned error at iter {}: {:?}", i, e);
            }
        }

        if cpu
            .bus()
            .get_npu_snapshot()
            .map(|s| s.tasks_done > 0)
            .unwrap_or(false)
        {
            done = true;
            break;
        }
    }

    if !done {
        dprintln!("Integration test timeout: NPU did not complete");
        dprintln!(
            "mcause: is_interrupt={} code={} mtval=0x{:08x}",
            cpu.csr().mcause.is_interrupt(),
            cpu.csr().mcause.code(),
            cpu.csr().mtval.get()
        );
        dprintln!("mepc (faulting PC): 0x{:08x}", cpu.csr().mepc.get().raw());
        dprintln!("Final PC: 0x{:08x}", cpu.pc().raw());
        if let Some(s) = cpu.bus().get_npu_snapshot() {
            dprintln!("NPU snapshot: desc_addr=0x{:x} desc_len={} tasks_done={} tasks_error={} pending_notify={}",
                s.desc_addr, s.desc_len, s.tasks_done, s.tasks_error, s.pending_desc_notify);

            if s.desc_addr != 0 {
                use mycpu::types::Addr;
                for i in 0..4 {
                    let a = Addr::new((s.desc_addr as u32).wrapping_add((i * 4) as u32));
                    match cpu.bus().read_word(a) {
                        Ok(w) => dprintln!("mem[0x{:08x}] = 0x{:08x}", a.raw(), w.raw()),
                        Err(e) => dprintln!("mem read err at 0x{:08x}: {:?}", a.raw(), e),
                    }
                }
            }
        } else {
            dprintln!("No NPU snapshot available");
        }
    }

    assert!(done, "ELF ran but NPU did not complete work in time");
}
