use core::{ffi::c_void, mem::size_of, slice};

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

#[derive(Debug)]
pub enum RsdpError {
    InvalidSignature,
    InvalidChecksum,
}

impl Rsdp {
    pub unsafe fn from_ptr(ptr: *const c_void) -> Result<&'static Self, RsdpError> {
        let s = ptr as *const Self;
        let s = &*s;
        if s.signature != RSDP_SIGNATURE {
            return Err(RsdpError::InvalidSignature);
        }

        let length = if s.revision > 0 {
            s.length as usize
        } else {
            RSDP_V1_SIZE
        };

        let bytes = unsafe { slice::from_raw_parts(s as *const Rsdp as *const u8, length) };
        let sum = bytes.iter().fold(0u8, |sum, &byte| sum.wrapping_add(byte));

        if sum != 0 {
            return Err(RsdpError::InvalidChecksum);
        }

        Ok(s)
    }

    #[inline]
    pub fn revision(&self) -> u8 {
        self.revision
    }

    pub fn rsdt_tables(&self) -> RsdtEntriesIterator {
        assert!(self.rsdt_ptr != 0);
        unsafe { RsdtEntriesIterator::new(self.rsdt_ptr as *const Rsdt) }
    }

    pub fn xsdt_tables(&self) -> XsdtEntriesIterator {
        assert!(self.xsdt_ptr != 0);
        unsafe { XsdtEntriesIterator::new(self.xsdt_ptr as *const Xsdt) }
    }
}

#[repr(C, packed)]
struct Rsdt {
    header: SdtHeader,
}

#[derive(Clone, Copy)]
pub struct RsdtEntriesIterator {
    index: usize,
    entries: &'static [u32],
}

impl RsdtEntriesIterator {
    unsafe fn new(rsdt: *const Rsdt) -> Self {
        let length = ((*rsdt).header.length as usize - size_of::<Rsdt>()) / size_of::<u32>();
        let entries = slice::from_raw_parts(rsdt.add(1) as *const u32, length);
        Self { index: 0, entries }
    }
}

impl Iterator for RsdtEntriesIterator {
    type Item = *const SdtHeader;

    fn next(&mut self) -> Option<Self::Item> {
        let ptr = self.entries.get(self.index).map(|p| *p as *const SdtHeader);
        self.index += 1;
        ptr
    }
}

#[repr(C, packed)]
struct Xsdt {
    header: SdtHeader,
}

#[derive(Clone, Copy)]
pub struct XsdtEntriesIterator {
    index: usize,
    entries: *const u64,
    length: usize,
}

impl XsdtEntriesIterator {
    unsafe fn new(xsdt: *const Xsdt) -> Self {
        let length = ((*xsdt).header.length as usize - size_of::<Xsdt>()) / size_of::<u64>();
        Self {
            index: 0,
            length,
            entries: xsdt.add(1) as *const u64,
        }
    }
}

impl Iterator for XsdtEntriesIterator {
    type Item = *const SdtHeader;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.length {
            return None;
        }
        let ptr = unsafe { self.entries.add(self.index).read_unaligned() };
        self.index += 1;
        Some(ptr as Self::Item)
    }
}
