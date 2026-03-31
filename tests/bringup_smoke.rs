use mycpu::cpu::csr::machine::medeleg_bits;
use mycpu::cpu::csr::CsrRegister;
use mycpu::cpu::csr::{csr_addr, exception_code};
use mycpu::{Addr, Bus, Cpu, PrivilegeLevel, Ram, RegIdx, Word};

const RAM_BASE: u32 = 0x8000_0000;
const RAM_SIZE: usize = 2 * 1024 * 1024;
const START_PC: u32 = RAM_BASE;

const ADDI_X1_X0_42: u32 = 0x02A0_0093;
const ADDI_X2_X1_1: u32 = 0x0010_8113;
const ECALL: u32 = 0x0000_0073;
const NOP: u32 = 0x0000_0013;

fn cpu_with_program(program: &[u32]) -> Cpu {
    let mut bus = Bus::new();
    let ram = Ram::new(RAM_SIZE);
    bus.attach_memory(Addr::new(RAM_BASE), ram, "RAM");

    for (index, instruction) in program.iter().enumerate() {
        let addr = Addr::new(START_PC + (index as u32) * 4);
        bus.write_word(addr, Word::new(*instruction))
            .expect("program write to RAM should succeed");
    }

    Cpu::with_pc(bus, Addr::new(START_PC))
}

#[test]
fn bringup_smoke_boot_stub_runs() {
    let mut cpu = cpu_with_program(&[ADDI_X1_X0_42, ADDI_X2_X1_1, NOP]);

    cpu.step().expect("step 1 should succeed");
    cpu.step().expect("step 2 should succeed");
    cpu.step().expect("step 3 should succeed");

    assert_eq!(cpu.registers().read(RegIdx::new(1)).raw(), 42);
    assert_eq!(cpu.registers().read(RegIdx::new(2)).raw(), 43);
    assert_eq!(cpu.pc().raw(), START_PC + 12);
}

#[test]
fn bringup_smoke_ecall_traps_to_machine_mode_by_default() {
    let mut cpu = cpu_with_program(&[ECALL]);
    let mtvec = START_PC + 0x100;

    cpu.csr_mut()
        .write(csr_addr::MTVEC, mtvec, PrivilegeLevel::Machine)
        .expect("mtvec write should succeed");

    cpu.step().expect("ecall step should be handled as trap");

    assert_eq!(cpu.privilege(), PrivilegeLevel::Machine);
    assert_eq!(cpu.pc().raw(), mtvec & !0x3);
    assert_eq!(cpu.csr().mcause.code(), exception_code::ECALL_MACHINE);
    assert_eq!(cpu.csr().mepc.read(), START_PC);
}

#[test]
fn bringup_smoke_ecall_user_delegates_to_supervisor() {
    let mut cpu = cpu_with_program(&[ECALL]);
    let stvec = START_PC + 0x200;

    cpu.csr_mut()
        .write(
            csr_addr::MEDELEG,
            medeleg_bits::UECL,
            PrivilegeLevel::Machine,
        )
        .expect("medeleg write should succeed");
    cpu.csr_mut()
        .write(csr_addr::STVEC, stvec, PrivilegeLevel::Machine)
        .expect("stvec write should succeed");

    cpu.set_privilege(PrivilegeLevel::User);

    cpu.step().expect("delegated ecall should trap to S-mode");

    assert_eq!(cpu.privilege(), PrivilegeLevel::Supervisor);
    assert_eq!(cpu.pc().raw(), stvec & !0x3);
    assert_eq!(cpu.csr().scause.code(), exception_code::ECALL_USER);
    assert_eq!(cpu.csr().sepc.read(), START_PC);
}
