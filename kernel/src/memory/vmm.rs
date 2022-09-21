use super::{pmm::PhysicalMemoryManager, PageAllocator, PhysicalAddress, VirtualAddress};
use crate::{
    memory::{pmm::PhysicalAllocError, PAGE_SIZE},
    read_cpu_reg,
};
use core::{fmt::Display, ptr, slice};
use log::trace;
use modular_bitfield::prelude::*;
use spin::{Mutex, MutexGuard};

static mut USER_ADDR_SPACE: Option<VirtualAddressSpace> = None;
static mut KERNEL_ADDR_SPACE: Option<VirtualAddressSpace> = None;
pub static mut VIRTUAL_MANAGER: Option<Mutex<VirtualMemoryManager>> = None;

pub const KERNEL_HEAP_START: usize = 0xFFFF_0080_0000_0000;
pub const KERNEL_HEAP_END: usize = 0xFFFF_0100_0000_0000; // size: 512 GB

pub type Result<T> = core::result::Result<T, VmmError>;

pub fn init(pmm: &'static mut PhysicalMemoryManager) {
    let ttbr0 = read_cpu_reg!("TTBR0_EL1");
    assert!(ttbr0 != 0);
    let ttbr1 = read_cpu_reg!("TTBR1_EL1");
    assert!(ttbr1 != 0);

    unsafe {
        USER_ADDR_SPACE = Some(VirtualAddressSpace::new(ttbr0 as *mut TableEntry, true));
        KERNEL_ADDR_SPACE = Some(VirtualAddressSpace::new(ttbr1 as *mut TableEntry, false));

        VIRTUAL_MANAGER = Some(Mutex::new(VirtualMemoryManager::new(pmm)))
    };
}

// safety: safe to call after init()
#[inline]
#[allow(unused)]
pub unsafe fn vmm() -> MutexGuard<'static, VirtualMemoryManager<'static>> {
    VIRTUAL_MANAGER.as_mut().unwrap().lock()
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

#[bitfield(bits = 12)]
#[derive(Debug, Clone, Copy, BitfieldSpecifier)]
struct UpperDescriptorAttributes {
    contigous: bool,
    #[allow(non_snake_case)]
    PXN: bool, // execute never at EL1
    #[allow(non_snake_case)]
    UXN: bool, // execute never at EL0
    #[skip]
    reserved: B4,
    #[skip]
    ignored: B5,
}

#[bitfield(bits = 10)]
#[derive(Debug, Clone, Copy, BitfieldSpecifier)]
struct LowerDescriptorAttributes {
    attr_index: B3, // MAIR index
    #[allow(non_snake_case)]
    non_secure: B1,
    #[allow(non_snake_case)]
    EL0_access: B1, // 0: no access in EL0 1: same access in EL0 and EL1 (defined by read only bit)
    readonly: B1,
    shareability: B2, // 00: non shareable 01: reserved 10: outer shareable 11: inner shareable
    access_flag: B1,
    non_global: B1,
}

#[bitfield(bits = 64)]
#[derive(Debug, Clone, Copy)]
struct BlockDescriptor {
    present: bool,
    block_or_table: B1, // should be 0 for block
    lower_attributes: LowerDescriptorAttributes,
    address: B36,
    #[skip]
    reserved: B4,
    upper_attributes: UpperDescriptorAttributes,
}

#[bitfield(bits = 64)]
#[derive(Debug, Clone, Copy)]
struct TableDescriptor {
    present: bool,
    block_or_table: B1, // should be 1
    #[skip]
    ignored: B10,
    address: B36,
    #[skip]
    reserved: B4,
    #[skip]
    ignored2: B7,

    // overrides
    #[allow(non_snake_case)]
    PXN: B1,
    #[allow(non_snake_case)]
    UXN: B1,
    #[allow(non_snake_case)]
    EL0_access: B1,
    readonly: B1,
    non_secure: B1,
}

#[derive(Clone, Copy)]
#[allow(unused)]
pub union TableEntry {
    bits: u64,
    block_descriptor: BlockDescriptor,
    table_descriptor: TableDescriptor,
}

impl TableEntry {
    fn create_table_descriptor(address: VirtualAddress) -> Self {
        debug_assert!(address & 0xFFF == 0, "Table address must be aligned to 4KB");
        let table = TableDescriptor::new()
            .with_present(true)
            .with_block_or_table(1)
            .with_address(((address & 0xFFFF_FFFF_FFFF) >> 12) as u64);
        TableEntry {
            table_descriptor: table,
        }
    }

    fn create_page_descriptor(
        address: PhysicalAddress,
        l_attrib: LowerDescriptorAttributes,
        u_attrib: UpperDescriptorAttributes,
    ) -> Self {
        debug_assert!(address & 0xFFF == 0, "Address must be aligned to 4KB");
        let bd = BlockDescriptor::new()
            .with_present(true)
            .with_block_or_table(1)
            .with_lower_attributes(l_attrib)
            .with_address(((address & 0xFFFF_FFFF_FFFF) >> 12) as u64)
            .with_upper_attributes(u_attrib);
        TableEntry {
            block_descriptor: bd,
        }
    }
}

#[derive(Debug)]
enum PageLevel {
    L0,
    L1,
    L2,
    L3,
}

impl From<usize> for PageLevel {
    fn from(l: usize) -> Self {
        match l {
            0 => Self::L0,
            1 => Self::L1,
            2 => Self::L2,
            3 => Self::L3,
            _ => panic!("Trying to create PageLevel enum from l > 3"),
        }
    }
}

#[inline]
fn get_page_level_index(addr: VirtualAddress, level: PageLevel) -> usize {
    match level {
        PageLevel::L0 => (addr >> 39) & 0x1FF,
        PageLevel::L1 => (addr >> 30) & 0x1FF,
        PageLevel::L2 => (addr >> 21) & 0x1FF,
        PageLevel::L3 => (addr >> 12) & 0x1FF,
    }
}

pub struct VirtualMemoryManager<'a> {
    physical: &'a mut PhysicalMemoryManager,
}

impl<'a> VirtualMemoryManager<'a> {
    pub fn new(physical: &'a mut PhysicalMemoryManager) -> Self {
        Self { physical }
    }

    // map virtual address "from" to physical address "to" and return "from"
    pub fn map_page(
        &mut self,
        from: VirtualAddress,
        to: PhysicalAddress,
        addr_space: Option<&mut VirtualAddressSpace>,
    ) -> Result<VirtualAddress> {
        trace!(target: "vmm", "Map {:p} to {:p}", from as *const u8, to as *const u8);
        let addr_space = addr_space.unwrap_or_else(|| get_current_addr_space(from));

        let mut current_level: &mut [TableEntry] = addr_space.ptr;
        let max_level = if addr_space.user { 2 } else { 3 };
        for l in 0..max_level {
            let entry = &mut current_level[get_page_level_index(from, PageLevel::from(l))];
            let entry_desc = unsafe { &entry.table_descriptor };
            if entry_desc.present() && entry_desc.block_or_table() == 0 {
                // if entry is a mapped block return already mapped error
                return Err(VmmError::AlreadyMapped);
            }
            current_level = unsafe {
                if entry_desc.present() {
                    slice::from_raw_parts_mut(
                        (entry_desc.address() << 12) as *mut TableEntry,
                        PAGE_SIZE / 8,
                    )
                } else {
                    let r = self.physical.alloc_page();
                    let page = match r {
                        Ok(p) => p,
                        Err(PhysicalAllocError::OutOfMemory) => return Err(VmmError::OutOfMemory),
                    };
                    ptr::write_bytes(page as *mut u8, 0, PAGE_SIZE); // clear page
                    *entry = TableEntry::create_table_descriptor(page);
                    slice::from_raw_parts_mut(page as *mut TableEntry, PAGE_SIZE / 8)
                }
            }
        }

        let entry = &mut current_level[get_page_level_index(from, PageLevel::L3)];
        let entry_desc = unsafe { &entry.table_descriptor };
        if entry_desc.present() {
            return Err(VmmError::AlreadyMapped);
        }
        let l_attrib = LowerDescriptorAttributes::new()
            .with_attr_index(1)
            .with_shareability(0b11)
            .with_access_flag(1);
        let u_attrib = UpperDescriptorAttributes::new();
        *entry = TableEntry::create_page_descriptor(to, l_attrib, u_attrib);

        Ok(from)
    }

    // unmap virtual address "addr" and return the physical address where it was mapped
    pub fn unmap_page(
        &mut self,
        addr: VirtualAddress,
        addr_space: Option<&mut VirtualAddressSpace>,
    ) -> Result<PhysicalAddress> {
        trace!(target: "vmm", "Unmap {:p}", addr as *const u8);
        let addr_space = addr_space.unwrap_or_else(|| get_current_addr_space(addr));

        let mut current_level: &mut [TableEntry] = addr_space.ptr;
        let max_level = if addr_space.user { 2 } else { 3 };
        for l in 0..max_level {
            let entry = &mut current_level[get_page_level_index(addr, PageLevel::from(l))];
            let entry_desc = unsafe { &entry.table_descriptor };

            if !entry_desc.present() || entry_desc.block_or_table() == 0 {
                return Err(VmmError::NotMapped);
            }

            current_level = unsafe {
                slice::from_raw_parts_mut(
                    (entry_desc.address() << 12) as *mut TableEntry,
                    PAGE_SIZE / 8,
                )
            };
        }

        let entry = &mut current_level[get_page_level_index(addr, PageLevel::L3)];
        let entry_desc = unsafe { &mut entry.table_descriptor };

        if !entry_desc.present() {
            return Err(VmmError::NotMapped);
        }

        let addr = entry_desc.address() << 12;
        entry_desc.set_present(false);

        Ok(addr as PhysicalAddress)
    }

    fn find_free_pages(
        &self,
        count: usize,
        usage: MemoryUsage,
        addr_space: Option<&VirtualAddressSpace>,
    ) -> Result<VirtualAddress> {
        trace!(target: "vmm", "Search {count} pages of {:?} virtual space", usage);
        let is_user_addr_space = match usage {
            MemoryUsage::KernelHeap => false,
        };
        let addr_space = if let Some(addr_space) = addr_space {
            if addr_space.user != is_user_addr_space {
                return Err(VmmError::InvalidAddrSpace);
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
        assert!(
            min_address % (1024 * 1024 * 1024) == 0 && max_address % (1024 * 1024 * 1024) == 0,
            "Virtual memory regions must be aligned to 1 GB"
        );
        assert!(max_address > min_address);
        assert!(
            max_address - min_address <= 512 * 1024 * 1024 * 1024,
            "Find free pages doesn't support search range larger than 512 GB"
        );

        let l1 = if is_user_addr_space {
            addr_space.ptr as &[TableEntry]
        } else {
            let entry = addr_space.ptr[get_page_level_index(min_address, PageLevel::L0)];
            let entry_desc = unsafe { &entry.table_descriptor };
            if entry_desc.present() && entry_desc.block_or_table() == 0 {
                // if entry is a mapped block return out of virtual space
                return Err(VmmError::OutOfVirtualSpace);
            }
            if entry_desc.present() {
                unsafe {
                    slice::from_raw_parts_mut(
                        (entry_desc.address() << 12) as *mut TableEntry,
                        PAGE_SIZE / 8,
                    )
                }
            } else {
                return Ok(min_address);
            }
        };

        let min_l1_index = get_page_level_index(min_address, PageLevel::L1);
        let mut max_l1_index = get_page_level_index(max_address, PageLevel::L1);
        if get_page_level_index(min_address, PageLevel::L0) + 1
            == get_page_level_index(max_address, PageLevel::L0)
        {
            if max_l1_index == 0 {
                max_l1_index = 511;
            }
        }

        let mut size = 0; // current consecutive free pages found
        let mut current_address = min_address;
        let mut start_addr = None;
        for index in min_l1_index..=max_l1_index {
            let entry = l1[index];
            let entry = unsafe { entry.table_descriptor };
            if !entry.present() {
                start_addr.get_or_insert(current_address);
                size += 262144; // 1 GB
                current_address += 262144 * PAGE_SIZE;
                if size >= count {
                    return Ok(start_addr.unwrap());
                }
                continue;
            }
            if entry.block_or_table() == 0 {
                // present block so unusable so reset size
                size = 0;
                start_addr = None;
                current_address += 262144 * PAGE_SIZE;
                continue;
            }

            // here l1_entry is a present table

            let l2 = unsafe {
                slice::from_raw_parts((entry.address() << 12) as *const TableEntry, PAGE_SIZE / 8)
            };

            for index in 0..512 {
                let entry = l2[index];
                let entry = unsafe { entry.table_descriptor };
                if !entry.present() {
                    start_addr.get_or_insert(current_address);
                    size += 512; // 12 MB
                    current_address += 512 * PAGE_SIZE;
                    if size >= count {
                        return Ok(start_addr.unwrap());
                    }
                    continue;
                }
                if entry.block_or_table() == 0 {
                    // present block so unusable so reset size
                    size = 0;
                    start_addr = None;
                    current_address += 512 * PAGE_SIZE;
                    continue;
                }

                let l3 = unsafe {
                    slice::from_raw_parts(
                        (entry.address() << 12) as *const TableEntry,
                        PAGE_SIZE / 8,
                    )
                };

                for index in 0..512 {
                    let entry = l3[index];
                    let entry = unsafe { entry.table_descriptor };
                    if !entry.present() {
                        start_addr.get_or_insert(current_address);
                        size += 1; // 4 KB
                        current_address += PAGE_SIZE;
                        if size >= count {
                            return Ok(start_addr.unwrap());
                        }
                        continue;
                    }

                    size = 0;
                    start_addr = None;
                    current_address += PAGE_SIZE;
                }
            }
        }

        Err(VmmError::OutOfVirtualSpace)
    }

    pub fn alloc_pages(
        &mut self,
        count: usize,
        usage: MemoryUsage,
        mut addr_space: Option<&mut VirtualAddressSpace>,
    ) -> Result<VirtualAddress> {
        trace!(target: "vmm", "Alloc {} pages of {:?}", count, usage);
        let virtual_addr =
            self.find_free_pages(count, usage, addr_space.as_ref().map_or(None, |a| Some(a)))?;
        for i in 0..count {
            let r = self.physical.alloc_page();
            let physical_addr = match r {
                Ok(addr) => addr,
                Err(PhysicalAllocError::OutOfMemory) => return Err(VmmError::OutOfMemory),
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
        &mut self,
        addr: VirtualAddress,
        count: usize,
        mut addr_space: Option<&mut VirtualAddressSpace>,
    ) -> Result<()> {
        trace!(target: "vmm", "Dealloc {} pages at addr {:p}", count, addr as *const u8);
        for i in 0..count {
            let phys_addr = self.unmap_page(
                addr + i * PAGE_SIZE,
                addr_space.as_mut().map_or(None, |a| Some(a)),
            )?;
            self.physical.unalloc_page(phys_addr);
        }
        Ok(())
    }
}

pub struct VmmPageAllocator<'a> {
    vmm: &'a Mutex<VirtualMemoryManager<'a>>,
}

impl<'a> VmmPageAllocator<'a> {
    pub fn new(vmm: &'a Mutex<VirtualMemoryManager<'a>>) -> Self {
        Self { vmm }
    }
}

impl<'a> PageAllocator for VmmPageAllocator<'a> {
   unsafe fn alloc(&self, count: usize) -> *mut u8 {
        let mut guard = self.vmm.lock();
        let r = guard.alloc_pages(count, MemoryUsage::KernelHeap, None);
        match r {
            Ok(addr) => addr as *mut u8,
            Err(_) => ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: usize, count: usize) {
        assert!(ptr % PAGE_SIZE == 0);
        self.vmm.lock().dealloc_pages(ptr, count, None).unwrap()
    }
}

pub struct VirtualAddressSpace {
    ptr: &'static mut [TableEntry], // the value in the TTBR register
    user: bool,                     // TTBR0 or TTBR1 (before or after hole)
}

impl VirtualAddressSpace {
    pub fn new(ptr: *mut TableEntry, user: bool) -> Self {
        debug_assert!(ptr.addr() != 0);
        Self {
            ptr: unsafe { slice::from_raw_parts_mut(ptr, 512) },
            user,
        }
    }
}

impl Display for VirtualAddressSpace {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "VirtualAddressSpace {{ ptr: {:p} }}", self.ptr.as_ptr())
    }
}

#[derive(Debug)]
pub enum VmmError {
    AlreadyMapped,
    NotMapped,
    OutOfMemory,
    OutOfVirtualSpace,
    InvalidAddrSpace,
}

#[derive(Debug)]
pub enum MemoryUsage {
    KernelHeap,
}
