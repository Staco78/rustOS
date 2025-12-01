use core::{arch::global_asm, fmt::Display, num::NonZeroU8, sync::atomic::Ordering};

use aarch64_cpu::registers::{DAIF, ESR_EL1, FAR_EL1, VBAR_EL1};
use log::{error, info, trace};
use tock_registers::interfaces::{Readable, Writeable};

use crate::{
    cpu::{self, InterruptFrame},
    scheduler::Cpu,
};

#[derive(Debug)]
enum CpuException {
    // some entries contains ESR_EL1
    Unkown(u32),
    NotImplemented(u32), // not implemented in the exception handler
    SvcInstruction,
    HvcInstruction,
    SmcInstruction,
    InstructionAbort(u64, u32),
    PCAlignment,
    DataAbort(u64, u32),
    StackAlignment,
    FloatingPointException,
    SError,
}

impl CpuException {
    fn from_esr(esr: u32) -> CpuException {
        let ec = esr >> 26;
        let far = FAR_EL1.get();
        match ec {
            0x00 => CpuException::Unkown(esr),
            0x11 => CpuException::SvcInstruction,
            0x12 => CpuException::HvcInstruction,
            0x13 => CpuException::SmcInstruction,
            0x15 => CpuException::SvcInstruction,
            0x16 => CpuException::HvcInstruction,
            0x17 => CpuException::SmcInstruction,
            0x20 | 0x21 => CpuException::InstructionAbort(far, esr),
            0x22 => CpuException::PCAlignment,
            0x24 | 0x25 => CpuException::DataAbort(far, esr),
            0x26 => CpuException::StackAlignment,
            0x28 | 0x2C => CpuException::FloatingPointException,
            0x2F => CpuException::SError,
            _ => CpuException::NotImplemented(esr),
        }
    }
}

impl Display for CpuException {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Cpu exception: ")?;
        match self {
            CpuException::Unkown(esr) => write!(f, "Unkown Exception ESR: {:#010X?}", esr),
            CpuException::SvcInstruction => write!(f, "SVC Instruction Exception"),
            CpuException::HvcInstruction => write!(f, "HvcInstruction Exception"),
            CpuException::SmcInstruction => write!(f, "SmcInstruction Exception"),
            CpuException::InstructionAbort(far, esr) => {
                write!(f, "Instruction Abort at {:p}: ", *far as *const ())?;
                let iss = esr & 0x1FFFFFF;
                let ll = iss & 0b11;
                match (iss >> 2) & 0b1111 {
                    0b0000 => write!(f, "Address size fault LL={ll}"),
                    0b0001 => write!(f, "Translation fault LL={ll}"),
                    0b0010 => write!(f, "Access Flag fault LL={ll}"),
                    0b0011 => write!(f, "Permission fault LL={ll}"),
                    0b0100 if ll == 0 => write!(f, "External abort"),
                    0b0110 if ll == 0 => write!(f, "Parity error"),
                    0b0101 => write!(f, "External abort on table walk LL={ll}"),
                    0b0111 if ll == 0 => write!(f, "Parity error on table walk LL={ll}"),
                    0b1000 if ll == 1 => write!(f, "Alignment fault"),
                    0b1100 if ll == 0 => write!(f, "TLB Conflict fault"),
                    _ => write!(f, "Unkown fault ESR_EL1: {esr:#X}"),
                }
            }
            CpuException::PCAlignment => write!(f, "PC Alignment Exception"),
            CpuException::DataAbort(far, esr) => {
                let iss = esr & 0x1FFFFFF;
                let ll = iss & 0b11;
                write!(f, "Data Abort at {:p}: ", *far as *const ())?;
                match (iss >> 2) & 0b1111 {
                    0b0000 => write!(f, "Address size fault LL={ll}"),
                    0b0001 => write!(f, "Translation fault LL={ll}"),
                    0b0010 => write!(f, "Access Flag fault LL={ll}"),
                    0b0011 => write!(f, "Permission fault LL={ll}"),
                    0b0100 if ll == 0 => write!(f, "External abort"),
                    0b0110 if ll == 0 => write!(f, "Parity error"),
                    0b0101 => write!(f, "External abort on table walk LL={ll}"),
                    0b0111 if ll == 0 => write!(f, "Parity error on table walk LL={ll}"),
                    0b1000 if ll == 1 => write!(f, "Alignment fault"),
                    0b1100 if ll == 0 => write!(f, "TLB Conflict fault"),
                    _ => write!(f, "Unkown fault ESR_EL1: {esr:#X}"),
                }?;
                write!(
                    f,
                    " caused by a {}",
                    if (iss >> 6) & 1 == 1 {
                        "write operation or cache maintenance/address translation"
                    } else {
                        "read operation"
                    }
                )
            }
            CpuException::StackAlignment => write!(f, "Stack Alignment Exception"),
            CpuException::FloatingPointException => write!(f, "Floating Point Exception"),
            CpuException::SError => write!(f, "SError Exception"),
            CpuException::NotImplemented(esr) => {
                let ec = esr >> 26;
                let iss = esr & 0x1FFFFFF;
                write!(
                    f,
                    "Unimplemented Exception: ESR_EL1: {esr:#X} EC: {ec:#X} ISS: {iss:#025b}"
                )
            }
        }
    }
}

global_asm!(include_str!("asm.S"));

unsafe extern "C" {
    #[allow(improper_ctypes)]
    unsafe static vector_table: ();
}

pub fn init() {
    // set vector table
    unsafe { VBAR_EL1.set((&vector_table as *const ()).addr() as u64) };

    info!("Exceptions initialized");
}

#[unsafe(no_mangle)]
unsafe extern "C" fn exception_handler(frame: *mut InterruptFrame) {
    let frame = unsafe { frame.as_mut() }.unwrap();
    error!(target: "panic", "Exception in CPU {}", cpu::id());
    error!(target: "panic", "{}", frame);
    panic!("{}", CpuException::from_esr(ESR_EL1.get() as u32));
}

#[unsafe(no_mangle)]
extern "C" fn interrupt_print(i: u32) {
    panic!("Received unwanted interrupt from vector {i}");
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DaifState(NonZeroU8);

impl DaifState {
    #[inline(always)]
    fn from_u64(v: u64) -> Self {
        debug_assert!(<u64 as TryInto<u8>>::try_into(v >> 6).is_ok());
        Self(unsafe { NonZeroU8::new_unchecked((v >> 6) as u8 | 1 << 7) })
    }

    #[inline(always)]
    fn into_u64(self) -> u64 {
        ((self.0.get() & 0x7F) << 6) as u64
    }

    #[inline(always)]
    pub fn all_masked(self) -> bool {
        self.0.get() & 0b1111 == 0b1111
    }
}

impl Display for DaifState {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "IrqState(")?;
        let val = self.0.get() & 0x7F;
        if val == 0b1111 {
            return write!(formatter, "All masked)");
        }
        let f = val & 0b0001 == 0;
        let i = val & 0b0010 == 0;
        let a = val & 0b0100 == 0;
        let d = val & 0b1000 == 0;
        if f {
            write!(formatter, "FIQ")?;
            if i {
                write!(formatter, ", ")?;
            }
        }
        if i {
            write!(formatter, "IRQ")?;
            if a {
                write!(formatter, ", ")?;
            }
        }
        if a {
            write!(formatter, "SError")?;
            if d {
                write!(formatter, ", ")?;
            }
        }
        if d {
            write!(formatter, "Debug")?;
        }
        write!(formatter, ")")
    }
}

/// return the value of the DAIF register before modification
#[inline]
pub fn disable_exceptions() -> DaifState {
    let v = DAIF.get();
    DAIF.write(DAIF::D::Masked + DAIF::A::Masked + DAIF::I::Masked + DAIF::F::Masked);
    DaifState::from_u64(v)
}

/// return the value of the DAIF register before modification
#[inline]
fn enable_exceptions() -> DaifState {
    let v = DAIF.get();
    DAIF.write(DAIF::D::Unmasked + DAIF::A::Unmasked + DAIF::I::Unmasked + DAIF::F::Unmasked);
    DaifState::from_u64(v)
}

/// Restore exceptions from a previous state.
#[inline]
pub fn restore_exceptions(state: DaifState) {
    DAIF.set(state.into_u64());
}

#[inline]
pub fn get_exception_state() -> DaifState {
    let v = DAIF.get();
    DaifState::from_u64(v)
}

/// Disable exceptions with depth level storage method
/// In depth level storage method, each CPU store
/// the number of times `disable_exceptions_depth()` was called more than `restore_exceptions_depth()`
#[inline]
pub fn disable_exceptions_depth() {
    let cpu = Cpu::current();
    let depth = cpu.irqs_depth.fetch_add(1, Ordering::Relaxed);
    if depth == 0 {
        disable_exceptions();
    }
    trace!(target: "exceptions", "Disable irqs with depth (new state: {}, new depth: {})", get_exception_state(), depth + 1);
}

#[inline]
pub fn restore_exceptions_depth() {
    let cpu = Cpu::current();
    let depth = cpu.irqs_depth.fetch_sub(1, Ordering::Relaxed);
    if depth == 1 {
        enable_exceptions();
    }
    trace!(target: "exceptions", "Restore irqs with depth (new state: {}, new depth: {})", get_exception_state(), depth - 1);
}

#[macro_export]
macro_rules! no_irq {
    ($inner:block) => {{
        let __daif_state = $crate::exceptions::disable_exceptions();
        $inner;
        $crate::exceptions::restore_exceptions(__daif_state);
    }};
}
