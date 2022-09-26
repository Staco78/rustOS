use super::{
    constants::ENTRIES_IN_TABLE, mmu::Mmu, PageAllocator, PhysicalAddress, VirtualAddress,
};
use crate::{
    memory::{
        constants::{
            KERNEL_HEAP_END, KERNEL_HEAP_START, PAGE_SIZE, PHYSICAL_LINEAR_MAPPING_END,
            PHYSICAL_LINEAR_MAPPING_START,
        },
        mmu::TableEntry,
    },
    read_cpu_reg,
};
use core::{fmt::Display, ptr, slice};
use log::trace;

static mut USER_ADDR_SPACE: Option<VirtualAddressSpace> = None;
static mut KERNEL_ADDR_SPACE: Option<VirtualAddressSpace> = None;
pub static mut VIRTUAL_MANAGER: Option<VirtualMemoryManager> = None;

pub fn init(pmm: &'static dyn PageAllocator) {
    let ttbr0 = read_cpu_reg!("TTBR0_EL1");
    assert!(ttbr0 != 0);
    let ttbr1 = read_cpu_reg!("TTBR1_EL1");
    assert!(ttbr1 != 0);

    unsafe {
        USER_ADDR_SPACE = Some(VirtualAddressSpace::new(ttbr0 as *mut TableEntry, true));
        KERNEL_ADDR_SPACE = Some(VirtualAddressSpace::new(ttbr1 as *mut TableEntry, false));

        VIRTUAL_MANAGER = Some(VirtualMemoryManager::new(pmm))
    };
}

#[inline]
pub fn phys_to_virt(addr: PhysicalAddress) -> VirtualAddress {
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

fn get_current_addr_space(addr: usize) -> &'static mut VirtualAddressSpace {
    if addr <= 0x0000_FFFF_FFFF_FFFF {
        get_user_addr_space()
    } else if addr >= 0xFFFF_0000_0000_0000 {
        get_kernel_addr_space()
    } else {
        panic!("Address out of the address space")
    }
}

#[inline]
fn get_kernel_addr_space() -> &'static mut VirtualAddressSpace {
    unsafe { KERNEL_ADDR_SPACE.as_mut().unwrap() }
}

#[inline]
fn get_user_addr_space() -> &'static mut VirtualAddressSpace {
    unsafe { USER_ADDR_SPACE.as_mut().unwrap() }
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
        let addr_space = addr_space.unwrap_or_else(|| get_current_addr_space(from));

        self.mmu.map(from, to, MapSize::Size4KB, addr_space)
    }

    // unmap virtual address "addr" and return the physical address where it was mapped
    pub fn unmap_page(
        &self,
        addr: VirtualAddress,
        addr_space: Option<&mut VirtualAddressSpace>,
    ) -> Result<PhysicalAddress, UnmapError> {
        trace!(target: "vmm", "Unmap {:p}", addr as *const u8);
        let addr_space = addr_space.unwrap_or_else(|| get_current_addr_space(addr));

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
            get_user_addr_space()
        } else {
            get_kernel_addr_space()
        };

        let (min_address, max_address) = match usage {
            MemoryUsage::KernelHeap => (KERNEL_HEAP_START, KERNEL_HEAP_END),
        };

        self.mmu
            .find_free_pages(count, min_address, max_address, addr_space)

        //     assert!(
        //         min_address % (1024 * 1024 * 1024) == 0 && max_address % (1024 * 1024 * 1024) == 0,
        //         "Virtual memory regions must be aligned to 1 GB"
        //     );
        //     assert!(max_address > min_address);
        //     assert!(
        //         max_address - min_address <= 512 * 1024 * 1024 * 1024,
        //         "Find free pages doesn't support search range larger than 512 GB"
        //     );

        //     let l1 = if is_user_addr_space {
        //         addr_space.ptr as &[TableEntry]
        //     } else {
        //         let entry = addr_space.ptr[get_page_level_index(min_address, PageLevel::L0)];
        //         let entry_desc = unsafe { &entry.table_descriptor };
        //         if entry_desc.present() && entry_desc.block_or_table() == 0 {
        //             // if entry is a mapped block return out of virtual space
        //             return Err(VmmError::OutOfVirtualSpace);
        //         }
        //         if entry_desc.present() {
        //             unsafe {
        //                 slice::from_raw_parts_mut(
        //                     (entry_desc.address() << 12) as *mut TableEntry,
        //                     PAGE_SIZE / 8,
        //                 )
        //             }
        //         } else {
        //             return Ok(min_address);
        //         }
        //     };

        //     let min_l1_index = get_page_level_index(min_address, PageLevel::L1);
        //     let mut max_l1_index = get_page_level_index(max_address, PageLevel::L1);
        //     if get_page_level_index(min_address, PageLevel::L0) + 1
        //         == get_page_level_index(max_address, PageLevel::L0)
        //     {
        //         max_l1_index = 511;
        //     }

        //     let mut size = 0; // current consecutive free pages found
        //     let mut current_address = min_address;
        //     let mut start_addr = None;
        //     for index in min_l1_index..=max_l1_index {
        //         let entry = l1[index];
        //         let entry = unsafe { entry.table_descriptor };
        //         if !entry.present() {
        //             start_addr.get_or_insert(current_address);
        //             size += 262144; // 1 GB
        //             current_address += 262144 * PAGE_SIZE;
        //             if size >= count {
        //                 return Ok(start_addr.unwrap());
        //             }
        //             continue;
        //         }
        //         if entry.block_or_table() == 0 {
        //             // present block so unusable so reset size
        //             size = 0;
        //             start_addr = None;
        //             current_address += 262144 * PAGE_SIZE;
        //             continue;
        //         }

        //         // here l1_entry is a present table

        //         let l2 = unsafe {
        //             slice::from_raw_parts((entry.address() << 12) as *const TableEntry, PAGE_SIZE / 8)
        //         };

        //         for index in 0..512 {
        //             let entry = l2[index];
        //             let entry = unsafe { entry.table_descriptor };
        //             if !entry.present() {
        //                 start_addr.get_or_insert(current_address);
        //                 size += 512; // 12 MB
        //                 current_address += 512 * PAGE_SIZE;
        //                 if size >= count {
        //                     return Ok(start_addr.unwrap());
        //                 }
        //                 continue;
        //             }
        //             if entry.block_or_table() == 0 {
        //                 // present block so unusable so reset size
        //                 size = 0;
        //                 start_addr = None;
        //                 current_address += 512 * PAGE_SIZE;
        //                 continue;
        //             }

        //             let l3 = unsafe {
        //                 slice::from_raw_parts(
        //                     (entry.address() << 12) as *const TableEntry,
        //                     PAGE_SIZE / 8,
        //                 )
        //             };

        //             for index in 0..512 {
        //                 let entry = l3[index];
        //                 let entry = unsafe { entry.table_descriptor };
        //                 if !entry.present() {
        //                     start_addr.get_or_insert(current_address);
        //                     size += 1; // 4 KB
        //                     current_address += PAGE_SIZE;
        //                     if size >= count {
        //                         return Ok(start_addr.unwrap());
        //                     }
        //                     continue;
        //                 }

        //                 size = 0;
        //                 start_addr = None;
        //                 current_address += PAGE_SIZE;
        //             }
        //         }
        //     }

        //     Err(VmmError::OutOfVirtualSpace)
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

// pub struct VmmPageAllocator<'a> {
//     vmm: &'a Mutex<VirtualMemoryManager<'a>>,
// }

// impl<'a> VmmPageAllocator<'a> {
//     pub fn new(vmm: &'a Mutex<VirtualMemoryManager<'a>>) -> Self {
//         Self { vmm }
//     }
// }

// impl<'a> PageAllocator for VmmPageAllocator<'a> {
//     unsafe fn alloc(&self, count: usize) -> *mut u8 {
//         let mut guard = self.vmm.lock();
//         let r = guard.alloc_pages(count, MemoryUsage::KernelHeap, None);
//         match r {
//             Ok(addr) => addr as *mut u8,
//             Err(_) => ptr::null_mut(),
//         }
//     }

//     unsafe fn dealloc(&self, ptr: usize, count: usize) {
//         assert!(ptr % PAGE_SIZE == 0);
//         self.vmm.lock().dealloc_pages(ptr, count, None).unwrap()
//     }
// }

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
        unsafe { slice::from_raw_parts(self.ptr as *const TableEntry, ENTRIES_IN_TABLE) }
    }

    #[inline]
    pub fn get_table_mut(&mut self) -> &mut [TableEntry] {
        unsafe { slice::from_raw_parts_mut(self.ptr, ENTRIES_IN_TABLE) }
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
}

#[derive(Debug)]
pub enum UnmapError {
    NotMapped,
    ParentMappedToBlock,
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
            MapError::AlreadyMapped => panic!("This should never happend"),
            MapError::PageAllocFailed => AllocError::OutOfMemory,
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
