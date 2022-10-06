// see https://uefi.org/sites/default/files/resources/ACPI_Spec_6_4_Jan22.pdf#page=195

use core::mem::size_of;

use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use static_assertions::assert_eq_size;

use super::sdt::SdtHeader;

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct Madt {
    pub header: SdtHeader,
    pub local_interrupt_controller_address: u32,
    pub flags: u32,
}

impl Madt {
    pub fn iter(&self) -> MadtIterator {
        MadtIterator {
            length: self.header.length - size_of::<Self>() as u32,
            base_ptr: (self as *const Self).addr() + size_of::<Self>(),
            pos: 0,
        }
    }
}

// iter over madt entries
pub struct MadtIterator {
    length: u32,
    base_ptr: usize,
    pos: u32,
}

impl Iterator for MadtIterator {
    type Item = &'static MadtEntryHeader;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.length {
            return None;
        }
        let ptr = (self.base_ptr + self.pos as usize) as *const MadtEntryHeader;
        self.pos += unsafe { (*ptr).length } as u32;

        let r = unsafe {
            MadtEntryType::from_u8((*ptr).struct_type as u8).expect("Invalid MADT struct type");
            ptr.as_ref().unwrap_unchecked()
        };

        Some(r)
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct MadtEntryHeader {
    pub struct_type: MadtEntryType,
    pub length: u8,
}

assert_eq_size!(MadtEntryHeader, u16);

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromPrimitive)]
pub enum MadtEntryType {
    LApic = 0,
    IOApic = 1,
    InterruptSourceOverride = 2,
    NMISource = 3,
    LApicNMI = 4,
    LAPICAddressOverride = 5,
    IOSApic = 6,
    LSApic = 7,
    PlatformInterruptSources = 8,
    #[allow(non_camel_case_types)]
    x2Apic = 9,
    #[allow(non_camel_case_types)]
    x2ApicNMI = 0xA,
    GICC = 0xB,
    GICD = 0xC,
    GicMsiFrame = 0xD,
    GIRC = 0xE,
    GicITS = 0xF,
    MultiprocessorWakeup = 0x10,
}

pub trait MadtTable {
    // this function is used to check if all field (particularly enums and signature) are valids
    fn from_header(header: &MadtEntryHeader) -> Option<&Self>;
}

// see https://uefi.org/sites/default/files/resources/ACPI_Spec_6_4_Jan22.pdf#page=205
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct GICC {
    header: MadtEntryHeader,
    __: u16,
    pub interface_number: u32,
    pub processor_uid: u32,
    pub flags: GiccFlags,
    pub parking_protocol_version: u32,
    pub gsiv: u32,
    pub parked_addr: u64,
    pub base_addr: u64,
    pub gicv: u64,
    pub gich: u64,
    pub vgic_maintenance: u32,
    pub gicr_addr: u64,
    pub mpidr: u64,
}

impl MadtTable for GICC {
    fn from_header(header: &MadtEntryHeader) -> Option<&Self> {
        if header.struct_type != MadtEntryType::GICC {
            return None;
        }

        unsafe {
            Some(
                (header as *const MadtEntryHeader as *const Self)
                    .as_ref()
                    .unwrap_unchecked(),
            )
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct GiccFlags {
    pub enabled: u8,
    pub performance_interrupt_mode: u8,
    pub vgic_maintenance_flags: u8,
    __: u8,
}

assert_eq_size!(GiccFlags, u32);

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct GICD {
    header: MadtEntryHeader,
    __: u16,
    pub gic_id: u32,
    pub base_address: u64,
    ___: u32,
    pub version: u8,
    reserved: [u8; 3],
}

impl MadtTable for GICD {
    fn from_header(header: &MadtEntryHeader) -> Option<&Self> {
        if header.struct_type != MadtEntryType::GICD {
            return None;
        }

        unsafe {
            Some(
                (header as *const MadtEntryHeader as *const Self)
                    .as_ref()
                    .unwrap_unchecked(),
            )
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct GicMsiFrame {
    header: MadtEntryHeader,
    __: u16,
    frame_id: u32,
    base_address: u64,
    flags: u32,
    spi_count: u16,
    spi_base: u16,
}

impl MadtTable for GicMsiFrame {
    fn from_header(header: &MadtEntryHeader) -> Option<&Self> {
        if header.struct_type != MadtEntryType::GicMsiFrame {
            return None;
        }

        unsafe {
            Some(
                (header as *const MadtEntryHeader as *const Self)
                    .as_ref()
                    .unwrap_unchecked(),
            )
        }
    }
}
