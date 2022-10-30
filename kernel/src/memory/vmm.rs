use super::{
    addr_space::VirtualAddressSpace, mmu::Mmu, AddrSpaceLock, AddrSpaceSelector, PageAllocator,
    PhysicalAddress, VirtualAddress,
};
use crate::{
    memory::{
        constants::{
            KERNEL_HEAP_END, KERNEL_HEAP_START, KERNEL_VIRT_SPACE_START, PAGE_SIZE,
            PHYSICAL_LINEAR_MAPPING_END, PHYSICAL_LINEAR_MAPPING_START, USER_SPACE_END,
            USER_SPACE_START, USER_VIRT_SPACE_END,
        },
        mmu::TableEntry,
    },
    scheduler::SCHEDULER,
    utils::sync_once_cell::SyncOnceCell,
};
use core::{fmt::Debug, ptr};
use cortex_a::registers::TTBR1_EL1;
use log::trace;

static mut DEFAULT_KERNEL_ADDR_SPACE: Option<AddrSpaceLock> = None;
pub static VIRTUAL_MANAGER: SyncOnceCell<VirtualMemoryManager> = SyncOnceCell::new();

pub fn init(pmm: &'static dyn PageAllocator) {
    unsafe {
        let ttbr1 = TTBR1_EL1.get_baddr();
        assert!(ttbr1 != 0);

        DEFAULT_KERNEL_ADDR_SPACE = Some(AddrSpaceLock::new(VirtualAddressSpace::new(
            ttbr1 as *mut TableEntry,
            false,
        )));

        VIRTUAL_MANAGER
            .set(VirtualMemoryManager::new(pmm))
            .expect("vmm::init called more than once");
    };
}

pub fn create_current_kernel_addr_space() -> AddrSpaceLock {
    let ttbr1 = TTBR1_EL1.get_baddr();
    assert!(ttbr1 != 0);
    unsafe { AddrSpaceLock::new(VirtualAddressSpace::new(ttbr1 as *mut TableEntry, false)) }
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
    VIRTUAL_MANAGER
        .get()
        .expect("VIRTUAL_MANAGER not initialized")
}

#[inline]
pub(super) fn get_kernel_addr_space<'a>() -> &'a AddrSpaceLock {
    SCHEDULER.try_get_kernel_process().map_or_else(
        || unsafe { DEFAULT_KERNEL_ADDR_SPACE.as_ref().expect("Vmm not inited") },
        |p| p.get_addr_space(),
    )
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
        addr_space: AddrSpaceSelector,
    ) -> Result<VirtualAddress, MapError> {
        trace!(target: "vmm", "Map {:p} to {:p}", from as *const (), to as *const ());
        if from >= USER_VIRT_SPACE_END && from < KERNEL_VIRT_SPACE_START {
            return Err(MapError::InvalidVirtualAddr);
        }

        let mut addr_space = addr_space.lock();
        {
            let is_user = from < USER_VIRT_SPACE_END;
            if addr_space.is_user != is_user {
                return Err(MapError::InvalidAddrSpace);
            }
        }
        self.mmu.map(from, to, options, &mut addr_space)
    }

    // unmap virtual address "addr" and return the physical address where it was mapped
    pub fn unmap_page(
        &self,
        addr: VirtualAddress,
        size: MapSize,
        addr_space: AddrSpaceSelector,
    ) -> Result<PhysicalAddress, UnmapError> {
        trace!(target: "vmm", "Unmap {:p}", addr as *const ());
        if addr >= USER_VIRT_SPACE_END && addr < KERNEL_VIRT_SPACE_START {
            return Err(UnmapError::InvalidVirtualAddr);
        }

        let mut addr_space = addr_space.lock();
        {
            let is_user = addr < USER_VIRT_SPACE_END;
            if addr_space.is_user != is_user {
                return Err(UnmapError::InvalidAddrSpace);
            }
        }
        self.mmu.unmap(addr, size, &mut addr_space)
    }

    fn find_free_pages(
        &self,
        count: usize,
        usage: MemoryUsage,
        addr_space: AddrSpaceSelector,
    ) -> Result<VirtualAddress, FindSpaceError> {
        trace!(target: "vmm", "Search {count} pages of {:?} virtual space", usage);
        let is_user_addr_space = match usage {
            MemoryUsage::KernelHeap => false,
            MemoryUsage::UserData => true,
        };

        let addr_space = addr_space.lock();
        if addr_space.is_user != is_user_addr_space {
            return Err(FindSpaceError::InvalidAddrSpace);
        }

        let (min_address, max_address) = match usage {
            MemoryUsage::KernelHeap => (KERNEL_HEAP_START, KERNEL_HEAP_END),
            MemoryUsage::UserData => (USER_SPACE_START, USER_SPACE_END),
        };

        self.mmu
            .find_free_pages(count, min_address, max_address, &addr_space)
    }

    pub fn alloc_pages(
        &self,
        count: usize,
        usage: MemoryUsage,
        addr_space: AddrSpaceSelector,
    ) -> Result<VirtualAddress, AllocError> {
        trace!(target: "vmm", "Alloc {} pages of {:?}", count, usage);

        let mut lock = addr_space.lock();
        let virtual_addr = self.find_free_pages(count, usage, AddrSpaceSelector::new(&mut lock))?;
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
                AddrSpaceSelector::new(&mut lock),
            )?;
        }

        Ok(virtual_addr)
    }

    pub fn dealloc_pages(
        &self,
        addr: VirtualAddress,
        count: usize,
        addr_space: AddrSpaceSelector,
    ) -> Result<(), DeallocError> {
        trace!(target: "vmm", "Dealloc {} pages at addr {:p}", count, addr as *const ());
        let mut lock = addr_space.lock();
        for i in 0..count {
            let phys_addr = self.unmap_page(
                addr + i * PAGE_SIZE,
                MapSize::Size4KB,
                AddrSpaceSelector::new(&mut lock),
            )?;
            unsafe { self.physical.dealloc(phys_addr, 1) };
        }
        Ok(())
    }
}

impl<'a> PageAllocator for VirtualMemoryManager<'a> {
    unsafe fn alloc(&self, count: usize) -> *mut u8 {
        match self.alloc_pages(count, MemoryUsage::KernelHeap, AddrSpaceSelector::kernel()) {
            Ok(addr) => addr as *mut u8,
            Err(_) => ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: usize, count: usize) {
        assert!(ptr % PAGE_SIZE == 0);
        self.dealloc_pages(ptr, count, AddrSpaceSelector::kernel())
            .unwrap()
    }
}

impl<'a> Debug for VirtualMemoryManager<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "VirtualMemoryManager")
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
    UserData,
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
