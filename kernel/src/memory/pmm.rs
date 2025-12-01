use crate::{
    error::{Error, MemoryError::*},
    memory::PAGE_SHIFT,
    sync::no_irq_locks::NoIrqMutex,
    utils::byte_size::ByteSize,
};

use super::{
    CustomMemoryTypes, PageAllocator, PhysicalAddress, address::Physical, constants::PAGE_SIZE,
};
use core::{fmt::Debug, slice};
use log::trace;
use uefi::{
    boot::MemoryType,
    mem::memory_map::{MemoryMap, MemoryMapRef},
};

pub static mut PHYSICAL_MANAGER: Option<NoIrqMutex<PhysicalMemoryManager>> = None;

pub fn init(memory_map: &MemoryMapRef<'static>) {
    unsafe { PHYSICAL_MANAGER = Some(NoIrqMutex::new(PhysicalMemoryManager::new(memory_map))) };
}

pub struct PhysicalMemoryManager {
    bitmap: &'static mut [u8],
}

impl PhysicalMemoryManager {
    pub fn new(memory_map: &MemoryMapRef) -> Self {
        let max_address = Self::get_max_address(&memory_map);
        let bitmap_size = max_address.addr() / PAGE_SIZE / 8;
        let bitmap_page_count = bitmap_size / PAGE_SIZE + (bitmap_size % PAGE_SIZE != 0) as usize;
        let bitmap_ptr = Self::get_free_space(&memory_map, bitmap_page_count)
            .expect("Cannot find free space for pmm bitmap");
        let bitmap = unsafe {
            slice::from_raw_parts_mut(
                bitmap_ptr.to_virt().as_ptr::<u8>(),
                bitmap_page_count * PAGE_SIZE,
            )
        };

        let mut s = Self { bitmap };
        s.init_bitmap(&memory_map);
        s.set_used_range(bitmap_ptr.addr() / PAGE_SIZE, bitmap_page_count);
        s.set_memory_map_usable(&memory_map);
        s
    }

    fn get_max_address(memory_map: &MemoryMapRef) -> PhysicalAddress {
        let mut max_address = 0;
        for desc in memory_map.entries() {
            if Self::is_memory_type_usable(desc.ty) {
                let addr = desc.phys_start as usize + desc.page_count as usize * PAGE_SIZE;
                if addr > max_address {
                    max_address = addr;
                }
            }
        }
        PhysicalAddress::new(max_address)
    }

    fn is_memory_type_usable(mem_type: MemoryType) -> bool {
        matches!(
            mem_type,
            MemoryType::CONVENTIONAL
                | MemoryType::BOOT_SERVICES_CODE
                | MemoryType::BOOT_SERVICES_DATA
                | MemoryType::LOADER_CODE
                | MemoryType::LOADER_DATA
        )
    }

    // find free space in memory map (used to find where to put the bitmap)
    fn get_free_space(memory_map: &MemoryMapRef, page_count: usize) -> Option<PhysicalAddress> {
        for desc in memory_map.entries() {
            if Self::is_memory_type_usable(desc.ty) && desc.page_count as usize >= page_count {
                return Some(PhysicalAddress::new(desc.phys_start as usize));
            }
        }
        None
    }

    fn init_bitmap(&mut self, memory_map: &MemoryMapRef) {
        trace!(target: "pmm", "Init bitmap");
        self.bitmap.fill(0xFF); // all used
        for desc in memory_map.entries() {
            if Self::is_memory_type_usable(desc.ty) {
                assert!(desc.phys_start as usize % PAGE_SIZE == 0);

                trace!(target: "pmm",
                    "Found usable memory at {:p} size: {}",
                    desc.phys_start as *const (),
                    ByteSize(desc.page_count as usize * PAGE_SIZE),
                );

                let start_page = desc.phys_start as usize / PAGE_SIZE;
                let end_page = start_page + desc.page_count as usize;
                let is_start_aligned = start_page % 8 == 0;
                let is_end_aligned = end_page % 8 == 0;

                let byte_start = if is_start_aligned {
                    start_page / 8
                } else {
                    self.set_free_range(start_page, 8 - (start_page % 8));
                    start_page / 8 + 1
                };

                let byte_end = if is_end_aligned {
                    end_page / 8
                } else {
                    self.set_free_range(end_page & !7, end_page % 8);
                    end_page / 8
                };

                if byte_start <= byte_end {
                    self.bitmap[byte_start..byte_end].fill(0);
                }
            }
        }
    }

    fn set_memory_map_usable(&mut self, memory_map: &MemoryMapRef) {
        let desc = memory_map
            .entries()
            .find(|desc| desc.ty.0 == CustomMemoryTypes::MemoryMap as u32)
            .expect("Memory map region not found");
        assert!(desc.phys_start as usize % PAGE_SIZE == 0);
        self.set_free_range(
            desc.phys_start as usize / PAGE_SIZE,
            desc.page_count as usize,
        );
    }

    #[inline]
    pub fn set_free(&mut self, index: usize) {
        self.bitmap[index / 8] &= !(0b10000000 >> (index % 8));
    }

    #[inline]
    pub fn set_used(&mut self, index: usize) {
        self.bitmap[index / 8] |= 0b10000000 >> (index % 8);
    }

    #[inline]
    pub fn set_free_range(&mut self, start: usize, length: usize) {
        for i in start..(start + length) {
            self.set_free(i);
        }
    }

    #[inline]
    pub fn set_used_range(&mut self, start: usize, length: usize) {
        for i in start..(start + length) {
            self.set_used(i);
        }
    }

    #[inline(always)]
    #[allow(unused)]
    pub fn is_used(&self, index: usize) -> bool {
        (self.bitmap[index / 8] & (0b10000000 >> (index % 8))) != 0
    }

    #[cfg(debug_assertions)]
    #[allow(unused)]
    pub fn print_bitmap(&self) {
        use log::debug;

        let len = self.bitmap.len() * 8;
        debug!(
            "Physical bitmap: {} pages {}",
            len,
            ByteSize(len * PAGE_SIZE)
        );
        let mut used = self.is_used(0);
        let mut from = 0;
        for i in 0..len {
            let u = self.is_used(i);
            if u != used {
                debug!(
                    "{} from {:p} to {:p} ({})",
                    if used { "Used" } else { "Free" },
                    (from * PAGE_SIZE) as *const (),
                    (i * PAGE_SIZE) as *const (),
                    ByteSize((i - from) * PAGE_SIZE)
                );
                used = u;
                from = i;
            }
        }
    }

    /// Find the byte index of the first byte which contains at least one 0 bit in the bitmap.
    #[inline]
    fn find_first_zero(bitmap: &[u8]) -> Option<usize> {
        let (a, bitmap64, b) = unsafe { bitmap.align_to::<u64>() };

        let mut index = 0;
        for val in a {
            if *val != u8::MAX {
                return Some(index);
            }
            index += 1;
        }

        for val in bitmap64 {
            if *val != u64::MAX {
                return Some(index + (u64::from_be(*val).leading_ones() / u8::BITS) as usize);
            }
            index += 8;
        }
        for val in b {
            if *val != u8::MAX {
                return Some(index);
            }
            index += 1;
        }

        None
    }

    /// Check if `count` pages are allocable in `bitmap`.
    /// The alloc is possible if there is `count` contigous 0 bits with at
    /// least 1 free bit in the first byte of the bitmap.
    ///
    /// `bitmap[0]` should contains at least 1 hole (0 bit).
    ///
    /// `offset` is the bit offset in `bitmap` where to start looking for alloc.
    ///
    /// If the alloc is possible, return `Ok` with the bit offset where the alloc start.
    /// Else, return `Err` with the count of bits which are checked to be unable
    /// to contains the alloc.
    #[inline]
    fn can_alloc(bitmap: &[u8], count: usize, offset: usize) -> Result<usize, usize> {
        debug_assert!(bitmap[0] != u8::MAX);
        debug_assert!(offset < u8::BITS as usize);
        debug_assert!(count > 0);

        let mask = !((1u16 << (u8::BITS as usize - offset)) - 1) as u8;
        let bit_i = (bitmap[0] | mask).leading_ones() as usize;
        if bit_i == u8::BITS as usize {
            return Err(u8::BITS as usize - offset);
        }
        if count == 1 {
            return Ok(bit_i);
        }

        let mask = !((1 << (u8::BITS as usize).saturating_sub(count)) - 1) >> bit_i;
        let val = bitmap[0];

        if (val & mask) == 0 {
            // if the alloc fit in the bitmap element (u8)
            if count <= u8::BITS as usize - bit_i {
                Ok(bit_i)
            } else {
                let remaining_count = count - (u8::BITS as usize - bit_i);

                // the count of elements that needs to be 0 for the alloc to succeed
                let full_elements = remaining_count / u8::BITS as usize;

                for (i, &v) in bitmap.iter().enumerate().skip(1).take(full_elements) {
                    if v != 0 {
                        return Err(i * u8::BITS as usize);
                    }
                }

                let remaining_bits = remaining_count % u8::BITS as usize;
                let mask = !(1 << ((u8::BITS as usize - remaining_bits) - 1));
                if bitmap[full_elements + 1] & mask == 0 {
                    Ok(bit_i)
                } else {
                    Err((full_elements + 1) * u8::BITS as usize)
                }
            }
        } else {
            Err(bit_i + (val << bit_i).leading_zeros() as usize)
        }
    }

    fn find_pages(&self, count: usize) -> Result<usize, Error> {
        debug_assert!(count > 0);

        let mut index = 0;
        let mut off = 0;

        loop {
            let bitmap = &self.bitmap[index..];
            let first_free =
                Self::find_first_zero(bitmap).ok_or(Error::Memory(OutOfPhysicalMemory))?;
            match Self::can_alloc(&bitmap[first_free..], count, off) {
                Ok(off) => return Ok((index + first_free) * u8::BITS as usize + off),
                Err(e) => {
                    index += e / u8::BITS as usize;
                    off += e % u8::BITS as usize;
                    if off >= u8::BITS as usize {
                        off -= u8::BITS as usize;
                        index += 1;
                    }
                }
            }
        }
    }

    pub fn alloc_pages(&mut self, count: usize) -> Result<PhysicalAddress, Error> {
        let pages = self.find_pages(count)?;
        self.set_used_range(pages, count);
        let addr = PhysicalAddress::new(pages << PAGE_SHIFT);
        trace!(target: "pmm", "Alloc {} page(s) at {}", count, addr);
        Ok(addr)
    }

    pub fn unalloc_pages(&mut self, addr: PhysicalAddress, count: usize) {
        assert!(addr.is_aligned_to(PAGE_SIZE));
        trace!(target: "pmm", "Dealloc {} page(s) at {}", count, addr);
        self.set_free_range(addr.addr() / PAGE_SIZE, count);
    }
}

pub struct PmmPageAllocator<'a> {
    pmm: &'a NoIrqMutex<PhysicalMemoryManager>,
}

impl<'a> PmmPageAllocator<'a> {
    pub fn new(pmm: &'a NoIrqMutex<PhysicalMemoryManager>) -> Self {
        Self { pmm }
    }
}

impl<'a> PageAllocator<Physical> for PmmPageAllocator<'a> {
    fn alloc(&self, count: usize) -> Option<PhysicalAddress> {
        match self.pmm.lock().alloc_pages(count) {
            Ok(addr) => Some(addr),
            Err(_) => None,
        }
    }

    unsafe fn dealloc(&self, ptr: PhysicalAddress, count: usize) {
        self.pmm.lock().unalloc_pages(ptr, count)
    }
}

impl<'a> Debug for PmmPageAllocator<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "PmmPageAllocator({:p})", self.pmm)
    }
}

unsafe impl<'a> Send for PmmPageAllocator<'a> {}
unsafe impl<'a> Sync for PmmPageAllocator<'a> {}
