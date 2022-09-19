use core::fmt::Display;

use log::error;

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
        unsafe {
            core::arch::asm!("wfi", options(nomem, nostack));
        }
    }
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
    pub fp: u64,
    pub lr: u64,
    pub xzr: u64,
    pub esr: u64,
    pub far: u64,
}

impl Display for InterruptFrame {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "Interrupt Frame:")?;
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
        writeln!(f, "fp:  {:#018X?}", self.fp)?;
        write!(f, "lr:  {:#018X?}    ", self.lr)?;
        write!(f, "esr: {:#018X?}    ", self.esr)?;
        write!(f, "far: {:#018X?}", self.far)
    }
}


#[macro_export]
macro_rules! read_cpu_reg {
    ($r:expr) => {
        unsafe {
            let mut o: u64;
            core::arch::asm!(concat!("mrs {}, ", $r), out(reg) o);
            o
        }
    };
}

#[macro_export]
macro_rules! write_cpu_reg {
    ($r:expr, $v:expr) => {
        unsafe {
            core::arch::asm!(concat!("msr ", $r, ", {}"), in(reg) $v as u64);
        }
    };
}