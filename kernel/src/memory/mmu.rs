use crate::memory::KERNEL_VIRT_SPACE_RANGE;

use super::{
    constants::{ENTRIES_IN_TABLE, PAGE_SIZE},
    vmm::{phys_to_virt, FindSpaceError, MapError, MapOptions, MapSize, UnmapError},
    PageAllocator, PhysicalAddress, VirtualAddress, VirtualAddressSpace,
};
use core::{arch::asm, fmt::Debug, ptr, slice, ops::Range};
use modular_bitfield::prelude::*;

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
    EL0_access: bool, // 0: no access in EL0 1: same access in EL0 and EL1 (defined by read only bit)
    readonly: bool,
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
    fn create_table_descriptor(address: PhysicalAddress) -> Self {
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

    fn create_block_descriptor(
        address: PhysicalAddress,
        l_attrib: LowerDescriptorAttributes,
        u_attrib: UpperDescriptorAttributes,
    ) -> Self {
        debug_assert!(address & 0xFFF == 0, "Address must be aligned to 4KB");
        let bd = BlockDescriptor::new()
            .with_present(true)
            .with_block_or_table(0)
            .with_lower_attributes(l_attrib)
            .with_address(((address & 0xFFFF_FFFF_FFFF) >> 12) as u64)
            .with_upper_attributes(u_attrib);
        TableEntry {
            block_descriptor: bd,
        }
    }

    #[inline]
    fn is_present(&self) -> bool {
        unsafe { self.block_descriptor.present() }
    }

    #[inline]
    fn addr(&self) -> PhysicalAddress {
        debug_assert!(self.is_present());
        unsafe { (self.block_descriptor.address() as usize) << 12 }
    }

    #[inline]
    fn is_block(&self) -> bool {
        debug_assert!(self.is_present());
        unsafe { self.block_descriptor.block_or_table() == 0 }
    }

    #[inline]
    fn set_present(&mut self, present: bool) {
        unsafe { self.block_descriptor.set_present(present) }
    }

    #[inline]
    fn unmap(&mut self) -> PhysicalAddress {
        assert!(self.is_present());
        let addr = self.addr();
        self.set_present(false);
        addr
    }
}

impl Debug for TableEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:#064b}", unsafe { self.bits })
    }
}

#[inline]
fn invalidate_addr(addr: VirtualAddress) {
    unsafe {
        let v = addr >> 12;
        asm!(
            "dsb ishst",
            "tlbi vaae1is, {}",
            "dsb ish",
            "isb",
            in(reg) v,
            options(preserves_flags)
        );
    }
}

#[inline]
pub fn invalidate_tlb_all() {
    unsafe {
        asm!(
            "dsb ishst",
            "tlbi vmalle1",
            "dsb ish",
            "isb",
            options(preserves_flags)
        )
    }
}

#[inline]
unsafe fn get_table(addr: *const TableEntry) -> &'static [TableEntry] {
    slice::from_raw_parts(addr, ENTRIES_IN_TABLE)
}

#[inline]
unsafe fn get_table_mut(addr: *mut TableEntry) -> &'static mut [TableEntry] {
    slice::from_raw_parts_mut(addr, ENTRIES_IN_TABLE)
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

pub struct Mmu<'a> {
    page_allocator: &'a dyn PageAllocator,
}

impl<'a> Mmu<'a> {
    pub fn new(page_allocator: &'a dyn PageAllocator) -> Self {
        Self { page_allocator }
    }

    #[inline]
    pub fn map(
        &self,
        from: VirtualAddress,
        to: PhysicalAddress,
        options: MapOptions,
        addr_space: &mut VirtualAddressSpace,
    ) -> Result<VirtualAddress, MapError> {
        match options.size {
            MapSize::Size4KB => self.map_4k(from, to, options, addr_space)?,
            MapSize::Size2MB => self.map_2m(from, to, options, addr_space)?,
            MapSize::Size1GB => self.map_1g(from, to, options, addr_space)?,
        }
        Ok(from)
    }

    fn map_4k(
        &self,
        from: VirtualAddress,
        to: PhysicalAddress,
        options: MapOptions,
        addr_space: &mut VirtualAddressSpace,
    ) -> Result<(), MapError> {
        let l1 = if addr_space.is_user {
            addr_space.get_table_mut()
        } else {
            let l0 = &mut addr_space.get_table_mut();
            self.create_next_table(&mut l0[get_page_level_index(from, PageLevel::L0)])
                .map_err(|e| {
                    debug_assert!(e != TableCreateError::AlreadyMappedToBlock);
                    e
                })?
        };
        let entry = &mut l1[get_page_level_index(from, PageLevel::L1)];
        if options.force_remap() && entry.is_present() && entry.is_block() {
            // if entry is a 1 GB block
            self.remap_block(entry, MapSize::Size1GB)?;
        }
        let l2 = self.create_next_table(entry)?;
        let entry = &mut l2[get_page_level_index(from, PageLevel::L2)];
        if options.force_remap() && entry.is_present() && entry.is_block() {
            // if entry is a 2 MB block
            self.remap_block(entry, MapSize::Size2MB)?;
        }
        let l3 = self.create_next_table(entry)?;

        let l3_entry = &mut l3[get_page_level_index(from, PageLevel::L3)];

        if l3_entry.is_present() && !options.force_remap() {
            return Err(MapError::AlreadyMapped);
        }

        let l_attrib = LowerDescriptorAttributes::new()
            .with_attr_index(1)
            .with_shareability(0b11)
            .with_access_flag(1);
        let u_attrib = UpperDescriptorAttributes::new();
        *l3_entry = TableEntry::create_page_descriptor(to, l_attrib, u_attrib);

        Ok(())
    }

    fn map_2m(
        &self,
        from: VirtualAddress,
        to: PhysicalAddress,
        options: MapOptions,
        addr_space: &mut VirtualAddressSpace,
    ) -> Result<(), MapError> {
        let l1 = if addr_space.is_user {
            addr_space.get_table_mut()
        } else {
            let l0 = &mut addr_space.get_table_mut();
            self.create_next_table(&mut l0[get_page_level_index(from, PageLevel::L0)])
                .map_err(|e| {
                    debug_assert!(e != TableCreateError::AlreadyMappedToBlock);
                    e
                })?
        };
        let entry = &mut l1[get_page_level_index(from, PageLevel::L1)];
        if options.force_remap() && entry.is_present() && entry.is_block() {
            // if entry is a 1 GB block
            self.remap_block(entry, MapSize::Size1GB)?;
        }
        let l2 = self.create_next_table(entry)?;

        let l2_entry = &mut l2[get_page_level_index(from, PageLevel::L2)];

        if l2_entry.is_present() && !options.force_remap() {
            return Err(MapError::AlreadyMapped);
        }

        let l_attrib = LowerDescriptorAttributes::new()
            .with_attr_index(1)
            .with_shareability(0b11)
            .with_access_flag(1);
        let u_attrib = UpperDescriptorAttributes::new();
        *l2_entry = TableEntry::create_block_descriptor(to, l_attrib, u_attrib);

        Ok(())
    }

    fn map_1g(
        &self,
        from: VirtualAddress,
        to: PhysicalAddress,
        options: MapOptions,
        addr_space: &mut VirtualAddressSpace,
    ) -> Result<(), MapError> {
        let l1 = if addr_space.is_user {
            addr_space.get_table_mut()
        } else {
            let l0 = &mut addr_space.get_table_mut();
            self.create_next_table(&mut l0[get_page_level_index(from, PageLevel::L0)])
                .map_err(|e| {
                    debug_assert!(e != TableCreateError::AlreadyMappedToBlock);
                    e
                })?
        };

        let l1_entry = &mut l1[get_page_level_index(from, PageLevel::L1)];

        if l1_entry.is_present() && !options.force_remap() {
            return Err(MapError::AlreadyMapped);
        }

        let l_attrib = LowerDescriptorAttributes::new()
            .with_attr_index(options.flags.attr_index())
            .with_shareability(options.flags.shareability())
            .with_EL0_access(options.flags.el0_access())
            .with_readonly(options.flags.read_only())
            .with_access_flag(1);
        let u_attrib = UpperDescriptorAttributes::new();
        *l1_entry = TableEntry::create_block_descriptor(to, l_attrib, u_attrib);

        Ok(())
    }

    fn create_next_table(
        &self,
        entry: &mut TableEntry,
    ) -> Result<&'static mut [TableEntry], TableCreateError> {
        if entry.is_present() {
            if entry.is_block() {
                return Err(TableCreateError::AlreadyMappedToBlock);
            }
            Ok(unsafe { get_table_mut(phys_to_virt(entry.addr()) as *mut TableEntry) })
        } else {
            unsafe {
                let page = self.page_allocator.alloc(1);
                if page.is_null() {
                    return Err(TableCreateError::PageAllocFailed);
                }
                ptr::write_bytes(phys_to_virt(page as usize) as *mut u8, 0, PAGE_SIZE); // clear page

                *entry = TableEntry::create_table_descriptor(page.addr());
                Ok(get_table_mut(phys_to_virt(entry.addr()) as *mut TableEntry))
            }
        }
    }

    // remap a block with 512 block of the lower size
    fn remap_block(&self, entry: &mut TableEntry, entry_size: MapSize) -> Result<(), MapError> {
        assert!(entry.is_present());
        assert!(entry.is_block());
        assert!(entry_size != MapSize::Size4KB);
        let table = unsafe { self.page_allocator.alloc(1) };
        if table.is_null() {
            return Err(MapError::PageAllocFailed);
        }
        unsafe {
            ptr::write_bytes(phys_to_virt(table.addr()) as *mut u8, 0, PAGE_SIZE);
        }
        let l2 = unsafe { get_table_mut(phys_to_virt(table.addr()) as *mut TableEntry) };
        let (l_attrib, u_attrib) = unsafe {
            (
                entry.block_descriptor.lower_attributes(),
                entry.block_descriptor.upper_attributes(),
            )
        };
        let mut to = entry.addr();
        for i in 0..ENTRIES_IN_TABLE {
            if entry_size == MapSize::Size1GB {
                l2[i] = TableEntry::create_block_descriptor(to, l_attrib, u_attrib);
                to += 2 * 1024 * 1024; // 2 MB
            } else {
                l2[i] = TableEntry::create_page_descriptor(to, l_attrib, u_attrib);
                to += 0x1000; // 4 KB
            }
        }

        *entry = TableEntry::create_table_descriptor(table.addr());

        Ok(())
    }

    #[inline]
    pub fn unmap(
        &self,
        addr: VirtualAddress,
        size: MapSize,
        addr_space: &mut VirtualAddressSpace,
    ) -> Result<PhysicalAddress, UnmapError> {
        match size {
            MapSize::Size4KB => self.unmap_4k(addr, addr_space),
            MapSize::Size2MB => self.unmap_2m(addr, addr_space),
            MapSize::Size1GB => self.unmap_1g(addr, addr_space),
        }
    }

    fn unmap_4k(
        &self,
        addr: VirtualAddress,
        addr_space: &mut VirtualAddressSpace,
    ) -> Result<PhysicalAddress, UnmapError> {
        let l1 = if addr_space.is_user {
            addr_space.get_table_mut()
        } else {
            let l0 = addr_space.get_table_mut();
            self.get_table(&mut l0[get_page_level_index(addr, PageLevel::L0)])?
        };
        let l2 = self.get_table(&mut l1[get_page_level_index(addr, PageLevel::L1)])?;
        let l3 = self.get_table(&mut l2[get_page_level_index(addr, PageLevel::L2)])?;
        let entry = &mut l3[get_page_level_index(addr, PageLevel::L3)];

        if !entry.is_present() {
            return Err(UnmapError::NotMapped);
        }

        let r_addr = entry.unmap();
        invalidate_addr(addr);

        Ok(r_addr)
    }

    fn unmap_2m(
        &self,
        addr: VirtualAddress,
        addr_space: &mut VirtualAddressSpace,
    ) -> Result<PhysicalAddress, UnmapError> {
        let l1 = if addr_space.is_user {
            addr_space.get_table_mut()
        } else {
            let l0 = addr_space.get_table_mut();
            self.get_table(&mut l0[get_page_level_index(addr, PageLevel::L0)])?
        };
        let l2 = self.get_table(&mut l1[get_page_level_index(addr, PageLevel::L1)])?;
        let entry = &mut l2[get_page_level_index(addr, PageLevel::L2)];

        if !entry.is_present() {
            return Err(UnmapError::NotMapped);
        }

        let r_addr = entry.unmap();
        invalidate_addr(addr);

        Ok(r_addr)
    }

    fn unmap_1g(
        &self,
        addr: VirtualAddress,
        addr_space: &mut VirtualAddressSpace,
    ) -> Result<PhysicalAddress, UnmapError> {
        let l1 = if addr_space.is_user {
            addr_space.get_table_mut()
        } else {
            let l0 = addr_space.get_table_mut();
            self.get_table(&mut l0[get_page_level_index(addr, PageLevel::L0)])?
        };
        let entry = &mut l1[get_page_level_index(addr, PageLevel::L1)];

        if !entry.is_present() {
            return Err(UnmapError::NotMapped);
        }

        let r_addr = entry.unmap();
        invalidate_addr(addr);

        Ok(r_addr)
    }

    fn get_table(&self, entry: &TableEntry) -> Result<&'static mut [TableEntry], TableGetError> {
        if !entry.is_present() {
            return Err(TableGetError::NotMapped);
        }
        if entry.is_block() {
            return Err(TableGetError::AlreadyMappedToBlock);
        }

        let addr = entry.addr();
        let addr = phys_to_virt(addr);
        Ok(unsafe { get_table_mut(addr as *mut TableEntry) })
    }

    // find free memory space of size "count * PAGE_SIZE" between min_addr and max_addr
    pub fn find_free_pages(
        &self,
        count: usize,
        range: Range<VirtualAddress>,
        addr_space: &VirtualAddressSpace,
    ) -> Result<VirtualAddress, FindSpaceError> {
        assert!(count > 0);
        assert!(!range.is_empty());
        assert!(count <= (range.end - range.start));
        assert!(range.start % PAGE_SIZE == 0);
        assert!(range.end % PAGE_SIZE == 0);
        assert!(
            range.start % (1024 * 1024 * 1024) == 0 && range.end % (1024 * 1024 * 1024) == 0,
            "Virtual memory regions must be aligned to 1 GB"
        );
        assert!(
            range.end - range.start <= 512 * 1024 * 1024 * 1024,
            "Find free pages doesn't support search range larger than 512 GB"
        );
        assert!(
            get_page_level_index(range.start, PageLevel::L0)
                == get_page_level_index(range.end - 1, PageLevel::L0)
        );

        let l0_index = get_page_level_index(range.start, PageLevel::L0);
        let min_l1_index = get_page_level_index(range.start, PageLevel::L1);
        let mut max_l1_index = get_page_level_index(range.end, PageLevel::L1);
        if max_l1_index == 0 {
            max_l1_index = 511;
        }

        let l1 = if addr_space.is_user {
            addr_space.get_table()
        } else {
            let l0 = addr_space.get_table();
            let r = self.get_table(&l0[l0_index]);
            let table = match r {
                Err(TableGetError::AlreadyMappedToBlock) => {
                    return Err(FindSpaceError::OutOfVirtualSpace)
                }
                Err(TableGetError::NotMapped) => return Ok(range.start),
                Ok(table) => table,
            };
            table
        };

        let page_off = if addr_space.is_user {
            0
        } else {
            KERNEL_VIRT_SPACE_RANGE.start / PAGE_SIZE
        };

        let mut found_pages = 0usize;
        let mut start_page = None;

        for l1_index in min_l1_index..=max_l1_index {
            let l1_entry = &l1[l1_index];
            if l1_entry.is_present() {
                if l1_entry.is_block() {
                    found_pages = 0;
                    start_page = None;
                } else {
                    let l2 =
                        unsafe { get_table(phys_to_virt(l1_entry.addr()) as *const TableEntry) };
                    for l2_index in 0..ENTRIES_IN_TABLE {
                        let l2_entry = &l2[l2_index];
                        if l2_entry.is_present() {
                            if l2_entry.is_block() {
                                found_pages = 0;
                                start_page = None;
                            } else {
                                let l3 = unsafe {
                                    get_table(phys_to_virt(l2_entry.addr()) as *const TableEntry)
                                };
                                for l3_index in 0..ENTRIES_IN_TABLE {
                                    let l3_entry = &l3[l3_index];
                                    if l3_entry.is_present() {
                                        found_pages = 0;
                                        start_page = None;
                                    } else {
                                        start_page.get_or_insert(
                                            ((l0_index * 512 + l1_index) * 512 + l2_index) * 512
                                                + l3_index
                                                + page_off,
                                        );
                                        found_pages += 1;
                                        if found_pages >= count {
                                            return Ok(start_page.unwrap() * PAGE_SIZE);
                                        }
                                    }
                                }
                            }
                        } else {
                            start_page.get_or_insert(
                                ((l0_index * 512 + l1_index) * 512 + l2_index) * 512 + page_off,
                            );
                            found_pages += 512;
                            if found_pages >= count {
                                return Ok(start_page.unwrap() * PAGE_SIZE);
                            }
                        }
                    }
                }
            } else {
                start_page.get_or_insert((l0_index * 512 + l1_index) * 512 * 512 + page_off);
                found_pages += 512 * 512;
                if found_pages >= count {
                    return Ok(start_page.unwrap() * PAGE_SIZE);
                }
            }
        }

        Err(FindSpaceError::OutOfVirtualSpace)
    }
}

#[derive(Debug, PartialEq, Eq)]
enum TableCreateError {
    AlreadyMappedToBlock,
    PageAllocFailed,
}

impl From<TableCreateError> for MapError {
    fn from(err: TableCreateError) -> Self {
        match err {
            TableCreateError::AlreadyMappedToBlock => MapError::AlreadyMapped,
            TableCreateError::PageAllocFailed => MapError::PageAllocFailed,
        }
    }
}

#[derive(Debug)]
enum TableGetError {
    NotMapped,
    AlreadyMappedToBlock,
}

impl From<TableGetError> for UnmapError {
    fn from(err: TableGetError) -> Self {
        match err {
            TableGetError::NotMapped => UnmapError::NotMapped,
            TableGetError::AlreadyMappedToBlock => UnmapError::ParentMappedToBlock,
        }
    }
}
