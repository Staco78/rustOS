use super::{
    constants::{ENTRIES_IN_TABLE, PAGE_SIZE},
    vmm::{phys_to_virt, FindSpaceError, MapError, MapSize, UnmapError, VirtualAddressSpace},
    PageAllocator, PhysicalAddress, VirtualAddress,
};
use core::{arch::asm, fmt::Debug, ptr, slice};
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
            .with_block_or_table(1)
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
fn get_table(addr: VirtualAddress) -> &'static [TableEntry] {
    unsafe { slice::from_raw_parts(addr as *const TableEntry, ENTRIES_IN_TABLE) }
}

#[inline]
fn get_table_mut(addr: VirtualAddress) -> &'static mut [TableEntry] {
    unsafe { slice::from_raw_parts_mut(addr as *mut TableEntry, ENTRIES_IN_TABLE) }
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
        size: MapSize,
        addr_space: &mut VirtualAddressSpace,
    ) -> Result<VirtualAddress, MapError> {
        match size {
            MapSize::Size4KB => self.map_4k(from, to, addr_space)?,
            MapSize::Size2MB => self.map_2m(from, to, addr_space)?,
            MapSize::Size1GB => self.map_1g(from, to, addr_space)?,
        }
        Ok(from)
    }

    fn map_4k(
        &self,
        from: VirtualAddress,
        to: PhysicalAddress,
        addr_space: &mut VirtualAddressSpace,
    ) -> Result<(), MapError> {
        let l1 = if addr_space.is_user {
            addr_space.get_table_mut()
        } else {
            let l0 = &mut addr_space.get_table_mut();
            self.create_next_table(&mut l0[get_page_level_index(from, PageLevel::L0)])?
        };
        let l2 = self.create_next_table(&mut l1[get_page_level_index(from, PageLevel::L1)])?;
        let l3 = self.create_next_table(&mut l2[get_page_level_index(from, PageLevel::L2)])?;

        let l3_entry = &mut l3[get_page_level_index(from, PageLevel::L3)];

        if l3_entry.is_present() {
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
        addr_space: &mut VirtualAddressSpace,
    ) -> Result<(), MapError> {
        let l1 = if addr_space.is_user {
            addr_space.get_table_mut()
        } else {
            let l0 = &mut addr_space.get_table_mut();
            self.create_next_table(&mut l0[get_page_level_index(from, PageLevel::L0)])?
        };
        let l2 = self.create_next_table(&mut l1[get_page_level_index(from, PageLevel::L1)])?;

        let l2_entry = &mut l2[get_page_level_index(from, PageLevel::L2)];

        if l2_entry.is_present() {
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
        addr_space: &mut VirtualAddressSpace,
    ) -> Result<(), MapError> {
        let l1 = if addr_space.is_user {
            addr_space.get_table_mut()
        } else {
            let l0 = &mut addr_space.get_table_mut();
            self.create_next_table(&mut l0[get_page_level_index(from, PageLevel::L0)])?
        };

        let l1_entry = &mut l1[get_page_level_index(from, PageLevel::L1)];

        if l1_entry.is_present() {
            return Err(MapError::AlreadyMapped);
        }

        let l_attrib = LowerDescriptorAttributes::new()
            .with_attr_index(1)
            .with_shareability(0b11)
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
            let addr = phys_to_virt(entry.addr());
            Ok(get_table_mut(addr))
        } else {
            unsafe {
                let page = self.page_allocator.alloc(1);
                if page.is_null() {
                    return Err(TableCreateError::PageAllocFailed);
                }
                ptr::write_bytes(phys_to_virt(page as usize) as *mut u8, 0, PAGE_SIZE); // clear page

                *entry = TableEntry::create_table_descriptor(page.addr());
                Ok(get_table_mut(phys_to_virt(entry.addr())))
            }
        }
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

        let r_addr = entry.addr();
        entry.set_present(false);
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

        let r_addr = entry.addr();
        entry.set_present(false);
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

        let r_addr = entry.addr();
        entry.set_present(false);
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
        Ok(get_table_mut(addr))
    }

    // find free memory space of size "count * PAGE_SIZE" between min_addr and max_addr
    pub fn find_free_pages(
        &self,
        count: usize,
        min_addr: VirtualAddress,
        max_addr: VirtualAddress,
        addr_space: &VirtualAddressSpace,
    ) -> Result<VirtualAddress, FindSpaceError> {
        assert!(count > 0);
        assert!(min_addr < max_addr);
        assert!(count <= (max_addr - min_addr));
        assert!(min_addr % PAGE_SIZE == 0);
        assert!(max_addr % PAGE_SIZE == 0);
        assert!(
            min_addr % (1024 * 1024 * 1024) == 0 && max_addr % (1024 * 1024 * 1024) == 0,
            "Virtual memory regions must be aligned to 1 GB"
        );
        assert!(
            max_addr - min_addr <= 512 * 1024 * 1024 * 1024,
            "Find free pages doesn't support search range larger than 512 GB"
        );
        assert!(
            get_page_level_index(min_addr, PageLevel::L0)
                == get_page_level_index(max_addr - 1, PageLevel::L0)
        );

        let l0_index = get_page_level_index(min_addr, PageLevel::L0);
        let min_l1_index = get_page_level_index(min_addr, PageLevel::L1);
        let mut max_l1_index = get_page_level_index(max_addr, PageLevel::L1);
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
                Err(TableGetError::NotMapped) => return Ok(min_addr),
                Ok(table) => table,
            };
            table
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
                    let l2 = get_table(phys_to_virt(l1_entry.addr()));
                    for l2_index in 0..ENTRIES_IN_TABLE {
                        let l2_entry = &l2[l2_index];
                        if l2_entry.is_present() {
                            if l2_entry.is_block() {
                                found_pages = 0;
                                start_page = None;
                            } else {
                                let l3 = get_table(phys_to_virt(l2_entry.addr()));
                                for l3_index in 0..ENTRIES_IN_TABLE {
                                    let l3_entry = &l3[l3_index];
                                    if l3_entry.is_present() {
                                        found_pages = 0;
                                        start_page = None;
                                    } else {
                                        found_pages += 1;
                                        if found_pages == 0 {
                                            debug_assert!(start_page.is_none());
                                            start_page =
                                                Some((l1_index * 512 + l2_index) * 512 + l3_index);
                                        }
                                        if found_pages >= count {
                                            return Ok(start_page.unwrap() * PAGE_SIZE);
                                        }
                                    }
                                }
                            }
                        } else {
                            found_pages += 512;
                            if found_pages == 0 {
                                debug_assert!(start_page.is_none());
                                start_page = Some((l1_index * 512 + l2_index) * 512);
                            }
                            if found_pages >= count {
                                return Ok(start_page.unwrap() * PAGE_SIZE);
                            }
                        }
                    }
                }
            } else {
                found_pages += 512 * 512;
                if found_pages == 0 {
                    debug_assert!(start_page.is_none());
                    start_page = Some(l1_index * 512 * 512);
                }
                if found_pages >= count {
                    return Ok(start_page.unwrap() * PAGE_SIZE);
                }
            }
        }

        Err(FindSpaceError::OutOfVirtualSpace)
    }
}

#[derive(Debug)]
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
