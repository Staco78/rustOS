use crate::utils::ByteSize;

use super::{constants::PAGE_SIZE, CustomMemoryTypes, PageAllocator, PhysicalAddress};
use core::{ptr, slice};
use log::trace;
use spin::{Mutex, MutexGuard};
use uefi::table::boot::{MemoryDescriptor, MemoryType};

pub static mut PHYSICAL_MANAGER: Option<Mutex<PhysicalMemoryManager>> = None;

pub fn init(memory_map: &'static [MemoryDescriptor]) {
    unsafe { PHYSICAL_MANAGER = Some(Mutex::new(PhysicalMemoryManager::new(memory_map))) };
}

// safety: safe to call after init()
#[inline]
#[allow(unused)]
pub unsafe fn physical() -> MutexGuard<'static, PhysicalMemoryManager> {
    PHYSICAL_MANAGER.as_mut().unwrap().lock()
}

pub struct PhysicalMemoryManager {
    bitmap: &'static mut [u8],
}

impl PhysicalMemoryManager {
    pub fn new(memory_map: &[MemoryDescriptor]) -> Self {
        let max_adddress = Self::get_max_address(memory_map);
        let bitmap_size = max_adddress / PAGE_SIZE / 8;
        let bitmap_page_count = bitmap_size / PAGE_SIZE + (bitmap_size % PAGE_SIZE != 0) as usize;
        let bitmap_ptr = Self::get_free_space(memory_map, bitmap_page_count)
            .expect("Cannot find free space for pmm bitmap");
        let bitmap = unsafe {
            slice::from_raw_parts_mut(
                bitmap_ptr as *mut u8,
                (bitmap_page_count * PAGE_SIZE) as usize,
            )
        };

        let mut s = Self { bitmap };
        s.init_bitmap(memory_map);
        s.set_used_range(bitmap_ptr / PAGE_SIZE, bitmap_page_count);
        s.set_memory_map_usable(memory_map);
        s
    }

    fn get_max_address(memory_map: &[MemoryDescriptor]) -> PhysicalAddress {
        let mut max_address = 0;
        for desc in memory_map {
            let addr = desc.phys_start as usize + desc.page_count as usize * PAGE_SIZE;
            if addr > max_address {
                max_address = addr;
            }
        }
        max_address
    }

    fn is_memory_type_usable(mem_type: MemoryType) -> bool {
        match mem_type {
            MemoryType::CONVENTIONAL => true,
            MemoryType::BOOT_SERVICES_CODE => true,
            MemoryType::BOOT_SERVICES_DATA => true,
            MemoryType::LOADER_CODE => true,
            MemoryType::LOADER_DATA => true,
            _ => false,
        }
    }

    // find free space in memory map (used for find where to put the bitmap)
    fn get_free_space(memory_map: &[MemoryDescriptor], page_count: usize) -> Option<usize> {
        for desc in memory_map {
            if Self::is_memory_type_usable(desc.ty) && desc.page_count as usize >= page_count {
                return Some(desc.phys_start as usize);
            }
        }
        None
    }

    fn init_bitmap(&mut self, memory_map: &[MemoryDescriptor]) {
        trace!(target: "pmm", "Init bitmap");
        self.bitmap.fill(0xFF); // all used
        for desc in memory_map {
            if Self::is_memory_type_usable(desc.ty) {
                assert!(desc.phys_start as usize % PAGE_SIZE == 0);

                trace!(target: "pmm",
                    "Found usable memory at {:p} size: {}",
                    desc.phys_start as *const u8,
                    ByteSize(desc.page_count as usize * PAGE_SIZE),
                );

                let start_page = desc.phys_start as usize / PAGE_SIZE;
                let end_page = start_page + desc.page_count as usize;
                let is_start_aligned = start_page % 8 == 0;
                let is_end_aligned = end_page % 8 == 0;

                let byte_start = if is_start_aligned {
                    start_page / 8
                } else {
                    self.set_free_range(start_page as usize, 8 - (start_page % 8) as usize);
                    start_page / 8 + 1
                };

                let byte_end = if is_end_aligned {
                    end_page / 8
                } else {
                    self.set_free_range((end_page & !7) as usize, end_page as usize % 8);
                    end_page / 8
                };

                if byte_start <= byte_end {
                    self.bitmap[byte_start as usize..byte_end as usize].fill(0);
                }
            }
        }
    }

    fn set_memory_map_usable(&mut self, memory_map: &[MemoryDescriptor]) {
        let desc = memory_map
            .iter()
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

    #[inline]
    pub fn is_used(&self, index: usize) -> bool {
        (self.bitmap[index / 8] & (0b10000000 >> (index % 8))) != 0
    }

    #[cfg(debug_assertions)]
    #[allow(unused)]
    pub fn print_bitmap(&self) {
        use log::debug;

        use crate::utils::ByteSize;

        let len = self.bitmap.len() * 8;
        debug!(
            "Physical bitmap: {} pages {}",
            len,
            ByteSize(len * PAGE_SIZE as usize)
        );
        let mut used = self.is_used(0);
        let mut from = 0;
        for i in 0..len {
            let u = self.is_used(i);
            if u != used {
                debug!(
                    "{} from {:p} to {:p} ({})",
                    if used { "Used" } else { "Free" },
                    (from * PAGE_SIZE) as *const u8,
                    (i * PAGE_SIZE) as *const u8,
                    ByteSize((i - from) * PAGE_SIZE)
                );
                used = u;
                from = i;
            }
        }
    }

    fn find_pages(&self, count: usize) -> Result<usize, PhysicalAllocError> {
        assert!(count > 0);
        let mut index = 0;
        let mut size = 0;
        for i in 0..(self.bitmap.len() * 8) {
            if !self.is_used(i) {
                if size == 0 {
                    index = i;
                }
                size += 1;
                if size == count {
                    return Ok(index);
                }
            } else {
                size = 0;
            }
        }
        Err(PhysicalAllocError::OutOfMemory)
    }

    pub fn alloc_pages(&mut self, count: usize) -> Result<PhysicalAddress, PhysicalAllocError> {
        let pages = self.find_pages(count)?;
        self.set_used_range(pages, count);
        trace!(target: "pmm", "Alloc {} page(s) at {:p}", count, (pages * PAGE_SIZE) as *const u8);
        Ok(pages * PAGE_SIZE)
    }

    pub fn unalloc_pages(&mut self, addr: PhysicalAddress, count: usize) {
        assert!(addr % PAGE_SIZE == 0);
        self.set_free_range(addr / PAGE_SIZE, count);
    }
}

#[derive(Debug)]
pub enum PhysicalAllocError {
    OutOfMemory,
}

pub struct PmmPageAllocator<'a> {
    pmm: &'a Mutex<PhysicalMemoryManager>,
}

impl<'a> PmmPageAllocator<'a> {
    pub fn new(pmm: &'a Mutex<PhysicalMemoryManager>) -> Self {
        Self { pmm }
    }
}

impl<'a> PageAllocator for PmmPageAllocator<'a> {
    unsafe fn alloc(&self, count: usize) -> *mut u8 {
        match self.pmm.lock().alloc_pages(count) {
            Ok(addr) => addr as *mut u8,
            Err(_) => ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: usize, count: usize) {
        self.pmm.lock().unalloc_pages(ptr, count)
    }
}
