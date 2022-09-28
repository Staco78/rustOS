use super::{
    constants::ENTRIES_IN_TABLE, mmu::Mmu, PageAllocator, PhysicalAddress, VirtualAddress,
};
use crate::{
    memory::{
        constants::{
            KERNEL_HEAP_END, KERNEL_HEAP_START, KERNEL_VIRT_SPACE_START, PAGE_SIZE,
            PHYSICAL_LINEAR_MAPPING_END, PHYSICAL_LINEAR_MAPPING_START, USER_VIRT_SPACE_END,
        },
        mmu::TableEntry,
    },
    read_cpu_reg,
};
use core::{fmt::Display, ptr, slice};
use log::trace;

static mut KERNEL_ADDR_SPACE: Option<VirtualAddressSpace> = None;
pub static mut VIRTUAL_MANAGER: Option<VirtualMemoryManager> = None;

pub fn init(pmm: &'static dyn PageAllocator) {
    unsafe {
        let ttbr1 = read_cpu_reg!("TTBR1_EL1");
        assert!(ttbr1 != 0);

        KERNEL_ADDR_SPACE = Some(VirtualAddressSpace::new(ttbr1 as *mut TableEntry, false));

        VIRTUAL_MANAGER = Some(VirtualMemoryManager::new(pmm))
    };
}

#[inline]
pub const fn phys_to_virt(addr: PhysicalAddress) -> VirtualAddress {
    assert!(addr < USER_VIRT_SPACE_END);
    let addr = addr + PHYSICAL_LINEAR_MAPPING_START;
    assert!(PHYSICAL_LINEAR_MAPPING_START <= addr && addr < PHYSICAL_LINEAR_MAPPING_END);
    addr
}

// safety: safe to call after init()
#[inline]
#[allow(unused)]
pub unsafe fn vmm() -> &'static VirtualMemoryManager<'static> {
    VIRTUAL_MANAGER.as_mut().unwrap()
}

#[inline]
fn get_kernel_addr_space() -> &'static mut VirtualAddressSpace {
    unsafe { KERNEL_ADDR_SPACE.as_mut().unwrap() }
}

#[allow(unused)]
#[derive(PartialEq, Eq)]
pub enum MapSize {
    Size4KB,
    Size2MB,
    Size1GB,
}

pub struct VirtualMemoryManager<'a> {
    physical: &'a dyn PageAllocator,
    mmu: Mmu<'a>,
}

impl<'a> VirtualMemoryManager<'a> {
    pub fn new(physical: &'a dyn PageAllocator) -> Self {
        Self {
            physical,
            mmu: Mmu::new(physical),
        }
    }

    // map virtual address "from" to physical address "to" and return "from"
    pub fn map_page(
        &self,
        from: VirtualAddress,
        to: PhysicalAddress,
        addr_space: Option<&mut VirtualAddressSpace>,
    ) -> Result<VirtualAddress, MapError> {
        trace!(target: "vmm", "Map {:p} to {:p}", from as *const u8, to as *const u8);
        if from >= USER_VIRT_SPACE_END && from < KERNEL_VIRT_SPACE_START {
            return Err(MapError::InvalidVirtualAddr);
        }
        let addr_space = match addr_space {
            Some(addr_space) => {
                let is_user = from < USER_VIRT_SPACE_END;
                if addr_space.is_user != is_user {
                    return Err(MapError::InvalidAddrSpace);
                }
                addr_space
            }
            None => {
                if from < KERNEL_VIRT_SPACE_START {
                    return Err(MapError::InvalidVirtualAddr);
                } else {
                    get_kernel_addr_space()
                }
            }
        };
        self.mmu.map(from, to, MapSize::Size4KB, addr_space)
    }

    // unmap virtual address "addr" and return the physical address where it was mapped
    pub fn unmap_page(
        &self,
        addr: VirtualAddress,
        addr_space: Option<&mut VirtualAddressSpace>,
    ) -> Result<PhysicalAddress, UnmapError> {
        trace!(target: "vmm", "Unmap {:p}", addr as *const u8);
        if addr >= USER_VIRT_SPACE_END && addr < KERNEL_VIRT_SPACE_START {
            return Err(UnmapError::InvalidVirtualAddr);
        }
        let addr_space = match addr_space {
            Some(addr_space) => {
                let is_user = addr < USER_VIRT_SPACE_END;
                if addr_space.is_user != is_user {
                    return Err(UnmapError::InvalidAddrSpace);
                }
                addr_space
            }
            None => {
                if addr < KERNEL_VIRT_SPACE_START {
                    return Err(UnmapError::InvalidVirtualAddr);
                } else {
                    get_kernel_addr_space()
                }
            }
        };

        self.mmu.unmap(addr, MapSize::Size4KB, addr_space)
    }

    fn find_free_pages(
        &self,
        count: usize,
        usage: MemoryUsage,
        addr_space: Option<&VirtualAddressSpace>,
    ) -> Result<VirtualAddress, FindSpaceError> {
        trace!(target: "vmm", "Search {count} pages of {:?} virtual space", usage);
        let is_user_addr_space = match usage {
            MemoryUsage::KernelHeap => false,
        };
        let addr_space = if let Some(addr_space) = addr_space {
            if addr_space.is_user != is_user_addr_space {
                return Err(FindSpaceError::InvalidAddrSpace);
            }
            addr_space
        } else if is_user_addr_space {
            return Err(FindSpaceError::InvalidAddrSpace);
        } else {
            get_kernel_addr_space()
        };

        let (min_address, max_address) = match usage {
            MemoryUsage::KernelHeap => (KERNEL_HEAP_START, KERNEL_HEAP_END),
        };

        self.mmu
            .find_free_pages(count, min_address, max_address, addr_space)
    }

    pub fn alloc_pages(
        &self,
        count: usize,
        usage: MemoryUsage,
        mut addr_space: Option<&mut VirtualAddressSpace>,
    ) -> Result<VirtualAddress, AllocError> {
        trace!(target: "vmm", "Alloc {} pages of {:?}", count, usage);
        let virtual_addr =
            self.find_free_pages(count, usage, addr_space.as_ref().map_or(None, |a| Some(a)))?;
        for i in 0..count {
            let r = unsafe { self.physical.alloc(1) };
            let physical_addr = if r.is_null() {
                return Err(AllocError::OutOfMemory);
            } else {
                r.addr()
            };
            self.map_page(
                virtual_addr + i * PAGE_SIZE,
                physical_addr,
                addr_space.as_mut().map_or(None, |a| Some(a)),
            )?;
        }

        Ok(virtual_addr)
    }

    pub fn dealloc_pages(
        &self,
        addr: VirtualAddress,
        count: usize,
        mut addr_space: Option<&mut VirtualAddressSpace>,
    ) -> Result<(), DeallocError> {
        trace!(target: "vmm", "Dealloc {} pages at addr {:p}", count, addr as *const u8);
        for i in 0..count {
            let phys_addr = self.unmap_page(
                addr + i * PAGE_SIZE,
                addr_space.as_mut().map_or(None, |a| Some(a)),
            )?;
            unsafe { self.physical.dealloc(phys_addr, 1) };
        }
        Ok(())
    }
}

impl<'a> PageAllocator for VirtualMemoryManager<'a> {
    unsafe fn alloc(&self, count: usize) -> *mut u8 {
        match self.alloc_pages(count, MemoryUsage::KernelHeap, None) {
            Ok(addr) => addr as *mut u8,
            Err(_) => ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: usize, count: usize) {
        assert!(ptr % PAGE_SIZE == 0);
        self.dealloc_pages(ptr, count, None).unwrap()
    }
}

pub struct VirtualAddressSpace {
    ptr: *mut TableEntry, // the value in the TTBR register
    pub is_user: bool,    // TTBR0 or TTBR1 (before or after hole)
}

impl VirtualAddressSpace {
    pub fn new(ptr: *mut TableEntry, user: bool) -> Self {
        debug_assert!(ptr.addr() != 0);
        Self { ptr, is_user: user }
    }

    #[inline]
    pub fn get_table(&self) -> &[TableEntry] {
        unsafe {
            slice::from_raw_parts(
                phys_to_virt(self.ptr as usize) as *const TableEntry,
                ENTRIES_IN_TABLE,
            )
        }
    }

    #[inline]
    pub fn get_table_mut(&mut self) -> &mut [TableEntry] {
        unsafe {
            slice::from_raw_parts_mut(
                phys_to_virt(self.ptr as usize) as *mut TableEntry,
                ENTRIES_IN_TABLE,
            )
        }
    }
}

impl Display for VirtualAddressSpace {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "VirtualAddressSpace {{ ptr: {:p}, is_user: {} }}",
            self.ptr, self.is_user
        )
    }
}

#[derive(Debug)]
pub enum MemoryUsage {
    KernelHeap,
}

#[derive(Debug)]
pub enum MapError {
    AlreadyMapped,
    PageAllocFailed,
    InvalidVirtualAddr,
    InvalidAddrSpace,
}

#[derive(Debug)]
pub enum UnmapError {
    NotMapped,
    ParentMappedToBlock,
    InvalidVirtualAddr,
    InvalidAddrSpace,
}

#[derive(Debug)]
pub enum FindSpaceError {
    InvalidAddrSpace,
    OutOfVirtualSpace,
}

#[derive(Debug)]
pub enum AllocError {
    OutOfMemory,
    InvalidAddrSpace,
    OutOfVirtualSpace,
}

impl From<MapError> for AllocError {
    fn from(err: MapError) -> Self {
        match err {
            MapError::PageAllocFailed => AllocError::OutOfMemory,
            MapError::AlreadyMapped => unreachable!(),
            MapError::InvalidVirtualAddr => unreachable!(),
            MapError::InvalidAddrSpace => unimplemented!()
        }
    }
}

impl From<FindSpaceError> for AllocError {
    fn from(err: FindSpaceError) -> Self {
        match err {
            FindSpaceError::InvalidAddrSpace => AllocError::InvalidAddrSpace,
            FindSpaceError::OutOfVirtualSpace => AllocError::OutOfVirtualSpace,
        }
    }
}

#[derive(Debug)]
pub enum DeallocError {
    NotAllocated,
}

impl From<UnmapError> for DeallocError {
    fn from(_: UnmapError) -> Self {
        DeallocError::NotAllocated
    }
}
