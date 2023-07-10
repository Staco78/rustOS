pub mod madt;
mod rsdp;
pub mod sdt;
pub mod spcr;

use crate::{
    acpi::sdt::Signature, error::Error, memory::PhysicalAddress,
    utils::sync_once_cell::SyncOnceCell,
};

use core::slice;

use static_assertions::assert_eq_size;
use uefi::table::cfg::{ConfigTableEntry, ACPI2_GUID, ACPI_GUID};

use self::rsdp::{AcpiIterator, Rsdp};

static RSDP: SyncOnceCell<Rsdp> = SyncOnceCell::new();

pub unsafe fn init(tables: &[ConfigTableEntry]) -> Result<(), Error> {
    let mut acpi = None;
    let mut acpi2 = None;
    for table in tables {
        if table.guid == ACPI_GUID {
            acpi = Some(
                PhysicalAddress::new(table.address.addr())
                    .to_virt()
                    .as_ptr(),
            );
        } else if table.guid == ACPI2_GUID {
            acpi2 = Some(
                PhysicalAddress::new(table.address.addr())
                    .to_virt()
                    .as_ptr(),
            );
        }
    }

    let rsdp = if let Some(acpi2) = acpi2 {
        unsafe { Rsdp::from_ptr(acpi2).unwrap() }
    } else if let Some(acpi) = acpi {
        unsafe { Rsdp::from_ptr(acpi).unwrap() }
    } else {
        return Err(Error::CustomStr("ACPI load: RDSP not found"));
    };

    unsafe { RSDP.set(rsdp).expect("Acpi already inited") };

    Ok(())
}

#[inline]
pub fn iter_tables() -> Option<AcpiIterator> {
    RSDP.get().map(|rsdp| rsdp.iter())
}

pub unsafe fn get_table<T>(signature: Signature) -> Option<&'static T> {
    if let Some(iter) = iter_tables() {
        for table in iter {
            if unsafe { (*table).signature == signature } {
                let r = (table as *const T).as_ref()?;
                let bytes = unsafe {
                    slice::from_raw_parts(table as usize as *const u8, (*table).length as usize)
                };
                let sum = bytes.iter().fold(0u8, |sum, &byte| sum.wrapping_add(byte));
                assert!(sum == 0);
                return Some(r);
            }
        }
    }
    None
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
