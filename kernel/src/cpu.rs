use core::{fmt::Display, panic::PanicInfo};

use cortex_a::{asm::wfi, registers::MPIDR_EL1};
use log::error;
use static_assertions::assert_eq_size;
use tock_registers::interfaces::Readable;

use crate::interrupts::exceptions::disable_exceptions;

#[panic_handler]
pub fn panic_handler(info: &PanicInfo) -> ! {
    if let Some(location) = info.location() {
        if let Some(message) = info.message() {
            error!(
                target: "panic",
                "Kernel panic in {} at ({}, {}): {}",
                location.file(),
                location.line(),
                location.column(),
                message
            );
        } else {
            error!(
                target: "panic",
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
        disable_exceptions();
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
    pub x0: usize,
    pub x1: usize,
    pub x2: usize,
    pub x3: usize,
    pub x4: usize,
    pub x5: usize,
    pub x6: usize,
    pub x7: usize,
    pub x8: usize,
    pub x9: usize,
    pub x10: usize,
    pub x11: usize,
    pub x12: usize,
    pub x13: usize,
    pub x14: usize,
    pub x15: usize,
    pub x16: usize,
    pub x17: usize,
    pub x18: usize,
    pub x19: usize,
    pub x20: usize,
    pub x21: usize,
    pub x22: usize,
    pub x23: usize,
    pub x24: usize,
    pub x25: usize,
    pub x26: usize,
    pub x27: usize,
    pub x28: usize,
    pub x29: usize,
    pub x30: usize, // lr
    pub sp: usize,

    pub pc: usize,
    pub pstate: usize,
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
