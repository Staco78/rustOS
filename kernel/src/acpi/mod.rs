pub mod madt;
mod rsdp;
pub mod sdt;
pub mod spcr;

use crate::{acpi::sdt::Signature, memory::vmm::phys_to_virt};

use core::{ffi::c_void, mem::MaybeUninit, slice};

use static_assertions::assert_eq_size;
use uefi::table::cfg::{ConfigTableEntry, ACPI2_GUID, ACPI_GUID};

use self::{
    rsdp::{Rsdp, RsdtEntriesIterator, XsdtEntriesIterator},
    sdt::SdtHeader,
};

pub struct AcpiParser {
    rsdt_iter: MaybeUninit<RsdtEntriesIterator>,
    xsdt_iter: MaybeUninit<XsdtEntriesIterator>,
    revision: u8,
}

impl AcpiParser {
    pub fn parse_tables(tables: &[ConfigTableEntry]) -> Result<Self, AcpiParsingError> {
        let mut acpi = None;
        let mut acpi2 = None;
        for table in tables {
            if table.guid == ACPI_GUID {
                acpi = Some(phys_to_virt(table.address.addr()) as *const c_void);
            } else if table.guid == ACPI2_GUID {
                acpi2 = Some(phys_to_virt(table.address.addr()) as *const c_void);
            }
        }

        let rsdp = if let Some(acpi2) = acpi2 {
            unsafe { Rsdp::from_ptr(acpi2).unwrap() }
        } else if let Some(acpi) = acpi {
            unsafe { Rsdp::from_ptr(acpi).unwrap() }
        } else {
            return Err(AcpiParsingError::RsdpNotFound);
        };

        let (rsdt_iter, xsdt_iter) = if rsdp.revision() > 0 {
            (MaybeUninit::uninit(), MaybeUninit::new(rsdp.xsdt_tables()))
        } else {
            (MaybeUninit::new(rsdp.rsdt_tables()), MaybeUninit::uninit())
        };

        Ok(Self {
            rsdt_iter,
            xsdt_iter,
            revision: rsdp.revision(),
        })
    }

    #[inline]
    fn get_iter(&mut self) -> &mut dyn Iterator<Item = *const SdtHeader> {
        unsafe {
            if self.revision > 0 {
                self.xsdt_iter.assume_init_mut() as &mut dyn Iterator<Item = *const SdtHeader>
            } else {
                self.rsdt_iter.assume_init_mut() as &mut dyn Iterator<Item = *const SdtHeader>
            }
        }
    }

    pub unsafe fn get_table<T>(&mut self, signature: Signature) -> Option<&T> {
        for table in self.get_iter() {
            if unsafe { (*table).signature == signature } {
                let r = (table as *const T).as_ref()?;
                let bytes = unsafe { slice::from_raw_parts(table as usize as *const u8, (*table).length as usize) };
                let sum = bytes.iter().fold(0u8, |sum, &byte| sum.wrapping_add(byte));
                assert!(sum == 0);
                return Some(r);
            }
        }
        None
    }
}

#[derive(Debug)]
pub enum AcpiParsingError {
    RsdpNotFound,
}

#[derive(Debug, Clone, Copy)]
#[allow(unused)]
pub enum AddressSpaceId {
    SystemMemory = 0,
    IO,
    PCI,
    EmbeddedController,
    SMBus,
    SystemCMOS,
    PciBarTarget,
    IPMI,
    GeneralPurposeIO,
    SerialBus,
    PCC,
    PRM,

    FunctionalFixedHardware = 0x7F,
}

assert_eq_size!(AddressSpaceId, u8);

#[derive(Debug, Clone, Copy)]
#[allow(unused)]
pub enum AccessSize {
    Undefined = 0,
    Byte,
    Word,
    DWord,
    QWord,
}

assert_eq_size!(AccessSize, u8);

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct AcpiGenericAddress {
    pub address_space_id: AddressSpaceId,
    pub register_bit_width: u8,
    pub register_bit_offset: u8,
    pub access_size: AccessSize,
    pub address: u64,
}
