use super::{
    addr_space::VirtualAddressSpace,
    address::{Physical, Virtual},
    mmu::Mmu,
    AddrSpaceLock, AddrSpaceSelector, PageAllocator, PhysicalAddress, VirtualAddress,
    MODULES_SPACE_RANGE,
};
use crate::{
    error::Error,
    error::MemoryError::*,
    memory::{constants::PAGE_SIZE, KERNEL_HEAP_RANGE, LOW_ADDR_SPACE_RANGE, USER_SPACE_RANGE},
    utils::sync_once_cell::SyncOnceCell,
};
use core::{
    fmt::Debug,
    sync::atomic::{AtomicUsize, Ordering},
};
use cortex_a::registers::TTBR1_EL1;
use log::trace;

static mut KERNEL_ADDR_SPACE: Option<AddrSpaceLock> = None;
pub static VIRTUAL_MANAGER: SyncOnceCell<VirtualMemoryManager> = SyncOnceCell::new();

pub fn init(pmm: &'static dyn PageAllocator<Physical>) {
    unsafe {
        KERNEL_ADDR_SPACE = Some(create_kernel_addr_space());

        VIRTUAL_MANAGER
            .set(VirtualMemoryManager::new(pmm))
            .expect("vmm::init called more than once");
    };
}

fn create_kernel_addr_space() -> AddrSpaceLock {
    let ttbr1 = TTBR1_EL1.get_baddr();
    assert!(ttbr1 != 0);
    unsafe {
        AddrSpaceLock::new_owned(VirtualAddressSpace::new(
            PhysicalAddress::new(ttbr1 as usize),
            false,
        ))
    }
}

#[inline]
pub fn vmm() -> &'static VirtualMemoryManager<'static> {
    VIRTUAL_MANAGER
        .get()
        .expect("VIRTUAL_MANAGER not initialized")
}

#[inline]
pub fn get_kernel_addr_space() -> &'static AddrSpaceLock {
    unsafe {
        KERNEL_ADDR_SPACE
            .as_ref()
            .expect("KERNEL_ADDR_SPACE not initialized")
    }
}

pub struct VirtualMemoryManager<'a> {
    physical: &'a dyn PageAllocator<Physical>,
    mmu: Mmu<'a>,
    modules_load_address: AtomicUsize,
}

impl<'a> VirtualMemoryManager<'a> {
    pub fn new(physical: &'a dyn PageAllocator<Physical>) -> Self {
        Self {
            physical,
            mmu: Mmu::new(physical),
            modules_load_address: AtomicUsize::new(MODULES_SPACE_RANGE.start.addr()),
        }
    }

    // map virtual address "from" to physical address "to" and return "from"
    pub fn map_page(
        &self,
        from: VirtualAddress,
        to: PhysicalAddress,
        options: MapOptions,
        addr_space: AddrSpaceSelector,
    ) -> Result<VirtualAddress, Error> {
        trace!(target: "vmm", "Map {} to {}", from, to);

        let mut addr_space = addr_space.lock();
        {
            let is_low = LOW_ADDR_SPACE_RANGE.contains(&from);
            if addr_space.is_low != is_low {
                return Err(Error::Memory(InvalidAddrSpace));
            }
        }
        self.mmu.map_page(from, to, options, &mut addr_space)
    }

    // unmap virtual address "addr" and return the physical address where it was mapped
    pub fn unmap_page(
        &self,
        addr: VirtualAddress,
        size: MapSize,
        addr_space: AddrSpaceSelector,
    ) -> Result<PhysicalAddress, Error> {
        trace!(target: "vmm", "Unmap {}", addr );

        let mut addr_space = addr_space.lock();
        {
            let is_low = LOW_ADDR_SPACE_RANGE.contains(&addr);
            if addr_space.is_low != is_low {
                return Err(Error::Memory(InvalidAddrSpace));
            }
        }
        self.mmu.unmap(addr, size, &mut addr_space)
    }

    pub fn find_free_pages(
        &self,
        count: usize,
        usage: MemoryUsage,
        addr_space: AddrSpaceSelector,
    ) -> Result<VirtualAddress, Error> {
        trace!(target: "vmm", "Search {count} pages of {:?} virtual space", usage);
        let is_low_addr_space = match usage {
            MemoryUsage::KernelHeap => false,
            MemoryUsage::ModuleSpace => false,
            MemoryUsage::UserData => true,
        };

        let addr_space = addr_space.lock();
        if addr_space.is_low != is_low_addr_space {
            return Err(Error::Memory(InvalidAddrSpace));
        }

        if usage == MemoryUsage::ModuleSpace {
            // module space alloc is special
            let addr = self
                .modules_load_address
                .fetch_add(count * PAGE_SIZE, Ordering::Relaxed);
            if addr + count * PAGE_SIZE >= MODULES_SPACE_RANGE.end {
                return Err(Error::Memory(OutOfVirtualSpace));
            }
            return Ok(VirtualAddress::new(addr));
        }

        let range = match usage {
            MemoryUsage::KernelHeap => KERNEL_HEAP_RANGE,
            MemoryUsage::UserData => USER_SPACE_RANGE,
            MemoryUsage::ModuleSpace => unreachable!(),
        };

        self.mmu.find_free_pages(count, range, &addr_space)
    }

    pub fn alloc_pages(
        &self,
        count: usize,
        usage: MemoryUsage,
        addr_space: AddrSpaceSelector,
    ) -> Result<VirtualAddress, Error> {
        trace!(target: "vmm", "Alloc {} pages of {:?}", count, usage);

        let mut lock = addr_space.lock();
        let virtual_addr =
            self.find_free_pages(count, usage, AddrSpaceSelector::Unlocked(&mut lock))?;
        for i in 0..count {
            let physical_addr = self
                .physical
                .alloc(1)
                .ok_or(Error::Memory(OutOfPhysicalMemory))?;

            self.map_page(
                virtual_addr + i * PAGE_SIZE,
                physical_addr,
                MapOptions::default_4k(),
                AddrSpaceSelector::Unlocked(&mut lock),
            )?;
        }

        Ok(virtual_addr)
    }

    pub fn dealloc_pages(
        &self,
        addr: VirtualAddress,
        count: usize,
        addr_space: AddrSpaceSelector,
    ) -> Result<(), Error> {
        trace!(target: "vmm", "Dealloc {} pages at addr {}", count, addr);
        let mut lock = addr_space.lock();
        for i in 0..count {
            let phys_addr = self.unmap_page(
                addr + i * PAGE_SIZE,
                MapSize::Size4KB,
                AddrSpaceSelector::Unlocked(&mut lock),
            )?;
            unsafe { self.physical.dealloc(phys_addr, 1) };
        }
        Ok(())
    }

    pub fn alloc_pages_at_addr(
        &self,
        addr: VirtualAddress,
        count: usize,
        flags: MapFlags,
        addr_space: AddrSpaceSelector,
    ) -> Result<VirtualAddress, Error> {
        let mut addr_space = addr_space.lock();
        let phys_addr = self
            .physical
            .alloc(count)
            .ok_or(Error::Memory(OutOfPhysicalMemory))?;

        Ok(self
            .mmu
            .map(addr, phys_addr, count, flags, &mut addr_space)?)
    }
}

impl<'a> PageAllocator<Virtual> for VirtualMemoryManager<'a> {
    fn alloc(&self, count: usize) -> Option<VirtualAddress> {
        match self.alloc_pages(count, MemoryUsage::KernelHeap, AddrSpaceSelector::kernel()) {
            Ok(addr) => Some(addr),
            Err(_) => None,
        }
    }

    unsafe fn dealloc(&self, ptr: VirtualAddress, count: usize) {
        assert!(ptr.is_aligned_to(PAGE_SIZE));
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
    pub fn default_rw(read_only: bool) -> Self {
        Self::new(read_only, false, 0b11, 1, false)
    }

    #[inline]
    pub fn force_remap(self) -> bool {
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
    pub fn default_size(size: MapSize) -> Self {
        Self {
            size,
            flags: Default::default(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MemoryUsage {
    KernelHeap,
    ModuleSpace,
    UserData,
}
