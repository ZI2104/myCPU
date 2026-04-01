//! Input MMIO peripheral for Linux/Framebuffer demo interaction.

use crate::error::{Result, SimError};
use crate::traits::Peripheral;
use crate::types::Addr;

/// Input peripheral base address.
pub const INPUT_BASE: u32 = 0x1000_2000;
/// Input peripheral MMIO region size.
pub const INPUT_SIZE: usize = 0x100;

const REG_KEY_STATE: u32 = 0x00;
const REG_LAST_EVENT: u32 = 0x04;
const REG_EVENT_COUNT: u32 = 0x08;
const REG_CONTROL: u32 = 0x0C;
const REG_STATUS: u32 = 0x10;

mod control_bits {
    pub const IRQ_EN: u32 = 1 << 0;
}

mod status_bits {
    pub const IRQ_PENDING: u32 = 1 << 0;
}

/// Simple input device model.
///
/// Key code range is mapped to a 32-bit key bitmap:
/// - bit N indicates whether key code N is currently pressed.
#[derive(Debug, Clone)]
pub struct InputDevice {
    base: Addr,
    key_state: u32,
    last_event: u32,
    event_count: u32,
    control: u32,
    status: u32,
}

impl Default for InputDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl InputDevice {
    /// Create input peripheral with default base address.
    pub fn new() -> Self {
        Self::with_base(Addr::new(INPUT_BASE))
    }

    /// Create input peripheral with custom base address.
    pub fn with_base(base: Addr) -> Self {
        Self {
            base,
            key_state: 0,
            last_event: 0,
            event_count: 0,
            control: 0,
            status: 0,
        }
    }

    /// Inject a key event from host.
    pub fn set_key(&mut self, key_code: u8, pressed: bool) {
        let bit = 1u32 << (key_code & 31);
        if pressed {
            self.key_state |= bit;
        } else {
            self.key_state &= !bit;
        }

        self.last_event = (u32::from(pressed) << 8) | key_code as u32;
        self.event_count = self.event_count.wrapping_add(1);

        if (self.control & control_bits::IRQ_EN) != 0 {
            self.status |= status_bits::IRQ_PENDING;
        }
    }

    /// Release all pressed keys.
    pub fn clear_keys(&mut self) {
        self.key_state = 0;
        if (self.control & control_bits::IRQ_EN) != 0 {
            self.status |= status_bits::IRQ_PENDING;
        }
    }

    /// Return a read-only snapshot for diagnostics.
    pub fn snapshot(&self) -> (u32, u32, u32, bool) {
        (
            self.key_state,
            self.last_event,
            self.event_count,
            (self.status & status_bits::IRQ_PENDING) != 0,
        )
    }

    fn read_reg(&self, reg: u32) -> u32 {
        match reg {
            REG_KEY_STATE => self.key_state,
            REG_LAST_EVENT => self.last_event,
            REG_EVENT_COUNT => self.event_count,
            REG_CONTROL => self.control,
            REG_STATUS => self.status,
            _ => 0,
        }
    }

    fn write_reg(&mut self, reg: u32, value: u32) {
        match reg {
            REG_KEY_STATE => self.key_state = value,
            REG_LAST_EVENT => self.last_event = value,
            REG_EVENT_COUNT => self.event_count = value,
            REG_CONTROL => self.control = value & control_bits::IRQ_EN,
            REG_STATUS => {
                // W1C for IRQ pending.
                if (value & status_bits::IRQ_PENDING) != 0 {
                    self.status &= !status_bits::IRQ_PENDING;
                }
            }
            _ => {}
        }
    }

    fn read_u8(&self, offset: u32) -> u8 {
        let reg = offset & !0x3;
        let shift = (offset & 0x3) * 8;
        ((self.read_reg(reg) >> shift) & 0xFF) as u8
    }

    fn write_u8(&mut self, offset: u32, value: u8) {
        let reg = offset & !0x3;
        let shift = (offset & 0x3) * 8;
        let mut current = self.read_reg(reg);
        current &= !(0xFF << shift);
        current |= (value as u32) << shift;
        self.write_reg(reg, current);
    }
}

impl Peripheral for InputDevice {
    fn read(&self, offset: Addr) -> Result<u8> {
        let off = offset.raw();
        if off >= INPUT_SIZE as u32 {
            return Err(SimError::InvalidAddress(offset));
        }
        Ok(self.read_u8(off))
    }

    fn write(&mut self, offset: Addr, value: u8) -> Result<()> {
        let off = offset.raw();
        if off >= INPUT_SIZE as u32 {
            return Err(SimError::InvalidAddress(offset));
        }
        self.write_u8(off, value);
        Ok(())
    }

    fn base_addr(&self) -> Addr {
        self.base
    }

    fn size(&self) -> usize {
        INPUT_SIZE
    }

    fn name(&self) -> &str {
        "Input"
    }

    fn has_interrupt(&self) -> bool {
        (self.status & status_bits::IRQ_PENDING) != 0
    }

    fn acknowledge_interrupt(&mut self) {
        self.status &= !status_bits::IRQ_PENDING;
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_u32(input: &InputDevice, reg: u32) -> u32 {
        let mut value = 0u32;
        for i in 0..4 {
            value |= (input.read(Addr::new(reg + i)).unwrap() as u32) << (i * 8);
        }
        value
    }

    fn write_u32(input: &mut InputDevice, reg: u32, value: u32) {
        for i in 0..4 {
            input
                .write(Addr::new(reg + i), ((value >> (i * 8)) & 0xFF) as u8)
                .unwrap();
        }
    }

    #[test]
    fn test_input_set_key_updates_state_and_event_count() {
        let mut input = InputDevice::new();
        input.set_key(2, true);

        assert_eq!(read_u32(&input, REG_KEY_STATE), 1 << 2);
        assert_eq!(read_u32(&input, REG_LAST_EVENT), 0x0102);
        assert_eq!(read_u32(&input, REG_EVENT_COUNT), 1);

        input.set_key(2, false);
        assert_eq!(read_u32(&input, REG_KEY_STATE), 0);
        assert_eq!(read_u32(&input, REG_LAST_EVENT), 0x0002);
        assert_eq!(read_u32(&input, REG_EVENT_COUNT), 2);
    }

    #[test]
    fn test_input_irq_pending_and_acknowledge() {
        let mut input = InputDevice::new();
        write_u32(&mut input, REG_CONTROL, control_bits::IRQ_EN);

        input.set_key(1, true);
        assert!(input.has_interrupt());
        assert_eq!(read_u32(&input, REG_STATUS) & status_bits::IRQ_PENDING, 1);

        input.acknowledge_interrupt();
        assert!(!input.has_interrupt());
    }

    #[test]
    fn test_input_clear_keys() {
        let mut input = InputDevice::new();
        input.set_key(0, true);
        input.set_key(3, true);
        assert_ne!(read_u32(&input, REG_KEY_STATE), 0);

        input.clear_keys();
        assert_eq!(read_u32(&input, REG_KEY_STATE), 0);
    }
}
