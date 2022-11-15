use core::{arch::global_asm, fmt::Display};

use cortex_a::registers::{DAIF, ESR_EL1, FAR_EL1, VBAR_EL1};
use log::{error, info};
use tock_registers::interfaces::{Readable, Writeable};

use crate::cpu::{self, InterruptFrame};

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

extern "C" {
    #[allow(improper_ctypes)]
    static vector_table: ();
}

pub fn init() {
    enable_irqs();

    // set vector table
    unsafe { VBAR_EL1.set((&vector_table as *const ()).addr() as u64) };

    info!("Exceptions initialized");
}

#[no_mangle]
unsafe extern "C" fn exception_handler(frame: *mut InterruptFrame) {
    let frame = frame.as_mut().unwrap();
    error!("Exception in CPU {}", cpu::id());
    error!("{}", frame);
    panic!("{}", CpuException::from_esr(ESR_EL1.get() as u32));
}

#[no_mangle]
extern "C" fn interrupt_print(i: u32) {
    panic!("Received unwanted interrupt from vector {i}");
}

// return the value of the DAIF register before modification
#[inline]
pub fn disable_irqs() -> u64 {
    let v = DAIF.get();
    DAIF.write(DAIF::D::Masked + DAIF::A::Masked + DAIF::I::Masked + DAIF::F::Masked);
    v
}

// return the value of the DAIF register before modification
#[inline]
pub fn enable_irqs() -> u64 {
    let v = DAIF.get();
    DAIF.write(DAIF::D::Unmasked + DAIF::A::Unmasked + DAIF::I::Unmasked + DAIF::F::Unmasked);
    v
}

// safety: v should come from disable_irqs()
#[inline]
pub unsafe fn restore_irqs(v: u64) {
    DAIF.set(v)
}

#[macro_export]
macro_rules! no_irq {
    ($inner:block) => {{
        let __daif_value = crate::exceptions::disable_irqs();
        $inner;
        unsafe { crate::exceptions::restore_irqs(__daif_value) };
    }};
}
