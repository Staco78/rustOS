use core::{arch::global_asm, fmt::Display};

use log::{debug, info};

use crate::cpu::InterruptFrame;

#[derive(Debug)]
enum CpuException {
    // some entries contains ESR_EL1
    Unkown(u32),
    NotImplemented(u32), // not implemented in the exception handler
    SvcInstruction,
    HvcInstruction,
    SmcInstruction,
    InstructionAbort(u32),
    PCAlignment,
    DataAbort(u32),
    StackAlignment,
    FloatingPointException,
    SError,
}

impl CpuException {
    fn from_esr(esr: u32) -> CpuException {
        let ec = esr >> 26;
        match ec {
            0x00 => CpuException::Unkown(esr),
            0x11 => CpuException::SvcInstruction,
            0x12 => CpuException::HvcInstruction,
            0x13 => CpuException::SmcInstruction,
            0x15 => CpuException::SvcInstruction,
            0x16 => CpuException::HvcInstruction,
            0x17 => CpuException::SmcInstruction,
            0x20 | 0x21 => CpuException::InstructionAbort(esr),
            0x22 => CpuException::PCAlignment,
            0x24 | 0x25 => CpuException::DataAbort(esr),
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
            CpuException::InstructionAbort(esr) => {
                write!(f, "Instruction Abort: ")?;
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
            CpuException::DataAbort(esr) => {
                let iss = esr & 0x1FFFFFF;
                let ll = iss & 0b11;
                write!(f, "Data Abort: ")?;
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
    fn init_ints();
}

pub fn init() {
    unsafe {
        init_ints();
    }
    info!("Exceptions initialized");
}

#[no_mangle]
unsafe extern "C" fn exception_handler(frame: *mut InterruptFrame) {
    let frame = frame.as_mut().unwrap();
    debug!("{}", frame);
    panic!("{}", CpuException::from_esr(frame.esr as u32));
}

#[no_mangle]
extern "C" fn interrupt_print(i: u32) {
    panic!("Received unwanted interrupt from vector {i}");
}
