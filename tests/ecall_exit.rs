use mycpu::{Addr, Bus, Cpu, Ram, RegIdx, Word};

// Verify that enabling the ECALL-as-exit feature causes an ECALL with
// a7==93 to halt the CPU instead of taking a trap.
#[test]
fn ecall_exit_halts_when_enabled() {
    let mut bus = Bus::new();
    let mut ram = Ram::new(0x1000);

    // ECALL instruction encoding: 0x00000073 (little-endian bytes)
    let ecall = [0x73u8, 0x00, 0x00, 0x00];
    ram.load(0, &ecall).unwrap();
    bus.attach_memory(Addr::new(0x0), ram, "test-ram");

    let mut cpu = Cpu::with_pc(bus, Addr::new(0x0));

    // Set a7 = 93 (Linux exit syscall number) and enable the flag
    cpu.registers_mut().write(RegIdx::new(17), Word::new(93));
    assert!(!cpu.is_halted());

    cpu.set_accept_ecall_exit(true);

    // Execute the ECALL instruction
    cpu.step().unwrap();

    assert!(cpu.is_halted(), "CPU should be halted when ECALL exit is enabled");
}

#[test]
fn ecall_without_flag_takes_trap() {
    let mut bus = Bus::new();
    let mut ram = Ram::new(0x1000);

    let ecall = [0x73u8, 0x00, 0x00, 0x00];
    ram.load(0, &ecall).unwrap();
    bus.attach_memory(Addr::new(0x0), ram, "test-ram");

    let mut cpu = Cpu::with_pc(bus, Addr::new(0x0));

    // Set a7 = 93 but do NOT enable the flag
    cpu.registers_mut().write(RegIdx::new(17), Word::new(93));
    cpu.set_accept_ecall_exit(false);

    cpu.step().unwrap();

    // Should not be halted; a trap should have been recorded
    assert!(!cpu.is_halted(), "CPU should not be halted when flag is disabled");
    assert!(cpu.first_trap().is_some(), "A trap should be recorded for ECALL when flag is disabled");
}
