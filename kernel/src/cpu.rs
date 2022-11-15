use core::fmt::Display;

use cortex_a::{asm::wfi, registers::MPIDR_EL1};
use log::error;
use static_assertions::assert_eq_size;
use tock_registers::interfaces::Readable;

use crate::interrupts::exceptions::disable_irqs;

#[panic_handler]
pub fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    if let Some(location) = info.location() {
        if let Some(message) = info.message() {
            error!(
                "Kernel panic in {} at ({}, {}): {}",
                location.file(),
                location.line(),
                location.column(),
                message
            );
        } else {
            error!(
                "Kernel panic in {} at ({}, {})",
                location.file(),
                location.line(),
                location.column(),
            );
        }
    }

    halt();
}

pub fn halt() -> ! {
    loop {
        disable_irqs();
        wfi();
    }
}

pub fn id() -> u32 {
    let id = MPIDR_EL1.get() & !0x80000000; // get cpu id from MPIDR reg and mask bit 31 which is always set
    id as u32
}

#[derive(Debug)]
#[repr(C)]
pub struct InterruptFrame {
    pub x0: u64,
    pub x1: u64,
    pub x2: u64,
    pub x3: u64,
    pub x4: u64,
    pub x5: u64,
    pub x6: u64,
    pub x7: u64,
    pub x8: u64,
    pub x9: u64,
    pub x10: u64,
    pub x11: u64,
    pub x12: u64,
    pub x13: u64,
    pub x14: u64,
    pub x15: u64,
    pub x16: u64,
    pub x17: u64,
    pub x18: u64,
    pub x19: u64,
    pub x20: u64,
    pub x21: u64,
    pub x22: u64,
    pub x23: u64,
    pub x24: u64,
    pub x25: u64,
    pub x26: u64,
    pub x27: u64,
    pub x28: u64,
    pub x29: u64,
    pub x30: u64, // lr
    pub sp: u64,

    pub pc: u64,
    pub pstate: u64,
}

assert_eq_size!(InterruptFrame, [u8; 272]); // stay consistent with the value in asm code

impl Display for InterruptFrame {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "Registers:")?;
        write!(f, "x0:  {:#018X?}    ", self.x0)?;
        writeln!(f, "x1:  {:#018X?}", self.x1)?;
        write!(f, "x2:  {:#018X?}    ", self.x2)?;
        writeln!(f, "x3:  {:#018X?}", self.x3)?;
        write!(f, "x4:  {:#018X?}    ", self.x4)?;
        writeln!(f, "x5:  {:#018X?}", self.x5)?;
        write!(f, "x6:  {:#018X?}    ", self.x6)?;
        writeln!(f, "x7:  {:#018X?}", self.x7)?;
        write!(f, "x8:  {:#018X?}    ", self.x8)?;
        writeln!(f, "x9:  {:#018X?}", self.x9)?;
        write!(f, "x10: {:#018X?}    ", self.x10)?;
        writeln!(f, "x11: {:#018X?}", self.x11)?;
        write!(f, "x12: {:#018X?}    ", self.x12)?;
        writeln!(f, "x13: {:#018X?}", self.x13)?;
        write!(f, "x14: {:#018X?}    ", self.x14)?;
        writeln!(f, "x15: {:#018X?}", self.x15)?;
        write!(f, "x16: {:#018X?}    ", self.x16)?;
        writeln!(f, "x17: {:#018X?}", self.x17)?;
        write!(f, "x18: {:#018X?}    ", self.x18)?;
        writeln!(f, "x19: {:#018X?}", self.x19)?;
        write!(f, "x20: {:#018X?}    ", self.x20)?;
        writeln!(f, "x21: {:#018X?}", self.x21)?;
        write!(f, "x22: {:#018X?}    ", self.x22)?;
        writeln!(f, "x23: {:#018X?}", self.x23)?;
        write!(f, "x24: {:#018X?}    ", self.x24)?;
        writeln!(f, "x25: {:#018X?}", self.x25)?;
        write!(f, "x26: {:#018X?}    ", self.x26)?;
        writeln!(f, "x27: {:#018X?}", self.x27)?;
        write!(f, "x28: {:#018X?}    ", self.x28)?;
        writeln!(f, "x29: {:#018X?}", self.x29)?;
        write!(f, "x30: {:#018X?}    ", self.x30)?;
        writeln!(f, "sp:  {:#018X?}", self.sp)?;
        write!(f, "pc:  {:#018X?} ", self.pc)?;
        write!(f, "pstate: {:#018X?}", self.pstate)
    }
}
