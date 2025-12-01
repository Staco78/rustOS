use core::{assert_matches::debug_assert_matches, mem::size_of, ptr, slice};

use crate::{error::Error, memory::PhysicalAddress};

use super::sdt::SdtHeader;

const RSDP_SIGNATURE: [u8; 8] = *b"RSD PTR ";
const RSDP_V1_SIZE: usize = 20;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Rsdp {
    signature: [u8; 8],
    checksum: u8,
    oemid: [u8; 6],
    revision: u8,
    rsdt_ptr: u32,

    // if revision >= 2
    length: u32,
    xsdt_ptr: u64,
    checksum2: u8,
    reserved: [u8; 3],
}

impl Rsdp {
    pub unsafe fn from_ptr(ptr: *const Self) -> Result<Self, Error> {
        let s: Self = unsafe { *ptr };
        if s.signature != RSDP_SIGNATURE {
            return Err(Error::CustomStr("Invalid RSDP signature"));
        }

        let length = if s.revision > 1 {
            s.length as usize
        } else {
            RSDP_V1_SIZE
        };

        let bytes = unsafe { slice::from_raw_parts(ptr as *const u8, length) };
        let sum = bytes.iter().fold(0u8, |sum, &byte| sum.wrapping_add(byte));

        if sum != 0 {
            return Err(Error::CustomStr("Invalid RSDP checksum"));
        }

        Ok(s)
    }

    #[inline]
    pub fn revision(&self) -> u8 {
        self.revision
    }

    pub fn iter(&self) -> AcpiIterator {
        if self.revision() < 2 {
            let ptr = PhysicalAddress::new(self.rsdt_ptr as usize)
                .to_virt()
                .as_ptr::<Rsdt>();
            assert!(!ptr.is_null());
            let header = unsafe { &(*ptr).header };
            let entries_size = header.length as usize - size_of::<Rsdt>();
            assert!(entries_size % 4 == 0);
            let ptr: *const u32 = unsafe { (&(*ptr).entries) as *const _ as *const u32 };
            let len = entries_size / 4;
            unsafe { AcpiIterator::new(ptr, len) }
        } else {
            let ptr = PhysicalAddress::new(self.xsdt_ptr as usize)
                .to_virt()
                .as_ptr::<Xsdt>();
            assert!(!ptr.is_null());
            let header = unsafe { &(*ptr).header };
            let entries_size = header.length as usize - size_of::<Xsdt>();
            assert!(entries_size % 8 == 0);
            let ptr: *const u64 = unsafe { (&(*ptr).entries) as *const _ as *const u64 };
            let len = entries_size / 8;
            unsafe { AcpiIterator::new(ptr, len) }
        }
    }
}

#[repr(C, packed)]
struct Rsdt {
    header: SdtHeader,
    entries: (),
}

type Xsdt = Rsdt;

pub struct AcpiIterator {
    entry_size: usize,
    entries: *const (),
    len: usize,
    index: usize,
}

impl AcpiIterator {
    unsafe fn new<T>(ptr: *const T, len: usize) -> Self {
        let size = size_of::<T>();
        debug_assert_matches!(size, 4 | 8);
        Self {
            entry_size: size,
            entries: ptr as *const (),
            len,
            index: 0,
        }
    }
}

impl Iterator for AcpiIterator {
    type Item = *const SdtHeader;
    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.len {
            return None;
        }

        let ptr = unsafe { self.entries.byte_add(self.index * self.entry_size) };
        let ptr = match self.entry_size {
            4 => {
                let value = unsafe { ptr::read_unaligned(ptr as *const u32) };
                value as usize
            }
            8 => {
                let value = unsafe { ptr::read_unaligned(ptr as *const u64) };
                value as usize
            }
            _ => unreachable!(),
        };

        self.index += 1;

        let ptr = PhysicalAddress::new(ptr).to_virt().as_ptr();
        Some(ptr)
    }
}
