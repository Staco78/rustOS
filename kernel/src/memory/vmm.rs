use super::{
    constants::ENTRIES_IN_TABLE, mmu::Mmu, PageAllocator, PhysicalAddress, VirtualAddress,
};
use crate::{memory::{
    constants::{
        KERNEL_HEAP_END, KERNEL_HEAP_START, KERNEL_VIRT_SPACE_START, PAGE_SIZE,
        PHYSICAL_LINEAR_MAPPING_END, PHYSICAL_LINEAR_MAPPING_START, USER_VIRT_SPACE_END,
    },
    mmu::TableEntry,
}, utils::sync_once_cell::SyncOnceCell};
use core::{fmt::{Display, Debug}, ptr, slice};
use cortex_a::registers::TTBR1_EL1;
use log::trace;

static mut KERNEL_ADDR_SPACE: Option<VirtualAddressSpace> = None;
// pub static mut VIRTUAL_MANAGER: Option<VirtualMemoryManager> = None;
pub static VIRTUAL_MANAGER: SyncOnceCell<VirtualMemoryManager> = SyncOnceCell::new();

pub fn init(pmm: &'static dyn PageAllocator) {
    unsafe {
        let ttbr1 = TTBR1_EL1.get_baddr();
        assert!(ttbr1 != 0);

        KERNEL_ADDR_SPACE = Some(VirtualAddressSpace::new(ttbr1 as *mut TableEntry, false));

        VIRTUAL_MANAGER.set(VirtualMemoryManager::new(pmm)).expect("vmm::init called more than once");
    };
}

#[inline]
pub const fn phys_to_virt(addr: PhysicalAddress) -> VirtualAddress {
    assert!(addr < USER_VIRT_SPACE_END);
    let addr = addr + PHYSICAL_LINEAR_MAPPING_START;
    assert!(PHYSICAL_LINEAR_MAPPING_START <= addr && addr < PHYSICAL_LINEAR_MAPPING_END);
    addr
}

#[inline]
pub fn vmm() -> &'static VirtualMemoryManager<'static> {
    VIRTUAL_MANAGER.get().expect("VIRTUAL_MANAGER not initialized")
}

#[inline]
fn get_kernel_addr_space() -> &'static mut VirtualAddressSpace {
    unsafe { KERNEL_ADDR_SPACE.as_mut().unwrap() }
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
        options: MapOptions,
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
        self.mmu.map(from, to, options, addr_space)
    }

    // unmap virtual address "addr" and return the physical address where it was mapped
    pub fn unmap_page(
        &self,
        addr: VirtualAddress,
        size: MapSize,
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

        self.mmu.unmap(addr, size, addr_space)
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
                MapOptions::default_4k(),
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
                MapSize::Size4KB,
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

impl<'a> Debug for VirtualMemoryManager<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "VirtualMemoryManager")
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

#[allow(unused)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapSize {
    Size4KB,
    Size2MB,
    Size1GB,
}

// bit[7]: remap (force remap and doesn't return AlreadyMapped)
// bits[6:4]: AttrIndx
// bits[3:2]: shareability
// bit[1]: EL0_access
// bit[0]: RO
#[derive(Debug, Clone, Copy)]
pub struct MapFlags(u8);

impl MapFlags {
    #[inline]
    pub fn new(
        read_only: bool,
        el0_access: bool,
        shareability: u8,
        attr_indx: u8,
        remap: bool,
    ) -> Self {
        assert!(shareability & 0b11 == shareability);
        assert!(attr_indx & 0b111 == attr_indx);
        Self(
            read_only as u8
                | (el0_access as u8) << 1
                | shareability << 2
                | attr_indx << 4
                | (remap as u8) << 7,
        )
    }

    #[inline]
    fn remap(self) -> bool {
        self.0 & 0b10000000 != 0
    }

    #[inline]
    pub fn attr_index(self) -> u8 {
        (self.0 & 0b01110000) >> 5
    }

    #[inline]
    pub fn shareability(self) -> u8 {
        (self.0 & 0b00001100) >> 2
    }

    #[inline]
    pub fn el0_access(self) -> bool {
        self.0 & 0b00000010 != 0
    }

    #[inline]
    pub fn read_only(self) -> bool {
        self.0 & 0b00000001 != 0
    }
}

impl Default for MapFlags {
    fn default() -> Self {
        Self(0b00011100) // remap: 0 AttrIndx: 1 shareability: 11 L0_access: 0 RO: 0
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MapOptions {
    pub size: MapSize,
    pub flags: MapFlags,
}

impl MapOptions {
    #[inline]
    pub fn new(size: MapSize, flags: MapFlags) -> Self {
        Self { size, flags }
    }

    #[inline]
    pub fn default_4k() -> Self {
        Self {
            size: MapSize::Size4KB,
            flags: Default::default(),
        }
    }

    #[inline]
    pub fn force_remap(self) -> bool {
        self.flags.remap()
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

#[derive(Debug, PartialEq, Eq)]
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
            MapError::InvalidAddrSpace => unimplemented!(),
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
