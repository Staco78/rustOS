use crate::error::{Error, MemoryError::*};
use crate::memory::{HIGH_ADDR_SPACE_RANGE, PAGE_SHIFT};

use super::{
    address::Physical,
    constants::{ENTRIES_IN_TABLE, PAGE_SIZE},
    vmm::{MapFlags, MapOptions, MapSize},
    PageAllocator, PhysicalAddress, VirtualAddress, VirtualAddressSpace,
};
use core::{arch::asm, fmt::Debug, mem::discriminant, ops::Range, ptr, slice};
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
        debug_assert!(
            address.is_aligned_to(PAGE_SIZE),
            "Table address must be aligned to 4KB"
        );
        let table = TableDescriptor::new()
            .with_present(true)
            .with_block_or_table(1)
            .with_address(((address.addr() & 0xFFFF_FFFF_FFFF) >> PAGE_SHIFT) as u64);
        TableEntry {
            table_descriptor: table,
        }
    }

    fn create_page_descriptor(
        address: PhysicalAddress,
        l_attrib: LowerDescriptorAttributes,
        u_attrib: UpperDescriptorAttributes,
    ) -> Self {
        debug_assert!(
            address.is_aligned_to(PAGE_SIZE),
            "Address must be aligned to 4KB"
        );
        let bd = BlockDescriptor::new()
            .with_present(true)
            .with_block_or_table(1)
            .with_lower_attributes(l_attrib)
            .with_address(((address.addr() & 0xFFFF_FFFF_FFFF) >> PAGE_SHIFT) as u64)
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
        debug_assert!(
            address.is_aligned_to(PAGE_SIZE),
            "Address must be aligned to 4KB"
        );
        let bd = BlockDescriptor::new()
            .with_present(true)
            .with_block_or_table(0)
            .with_lower_attributes(l_attrib)
            .with_address(((address.addr() & 0xFFFF_FFFF_FFFF) >> PAGE_SHIFT) as u64)
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
        let v = unsafe { (self.block_descriptor.address() as usize) << 12 };
        PhysicalAddress::new(v)
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
        let v = addr.addr() >> PAGE_SHIFT;
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
    let addr = addr.addr();
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
    page_allocator: &'a dyn PageAllocator<Physical>,
}

impl<'a> Mmu<'a> {
    pub fn new(page_allocator: &'a dyn PageAllocator<Physical>) -> Self {
        Self { page_allocator }
    }

    pub fn map(
        &self,
        from: VirtualAddress,
        to: PhysicalAddress,
        count: usize,
        flags: MapFlags,
        addr_space: &mut VirtualAddressSpace,
    ) -> Result<VirtualAddress, Error> {
        assert!(from.is_aligned_to(PAGE_SIZE));
        assert!(to.is_aligned_to(PAGE_SIZE));

        let max_vaddr = from + count * PAGE_SIZE;

        let mut vaddr = from;
        let mut paddr = to;
        while vaddr < max_vaddr {
            let remaining = max_vaddr - vaddr;
            // if aligned to 1GB
            if remaining >= 0x40000000 && vaddr % 0x40000000 == 0 && paddr % 0x40000000 == 0 {
                self.map_1g(vaddr, paddr, flags, addr_space)?;
                vaddr += 0x40000000;
                paddr += 0x40000000;
            }
            // if aligned to 2MB
            else if remaining >= 0x200000 && vaddr % 0x200000 == 0 && paddr % 0x200000 == 0 {
                self.map_2m(vaddr, paddr, flags, addr_space)?;
                vaddr += 0x200000;
                paddr += 0x200000;
            } else {
                self.map_4k(vaddr, paddr, flags, addr_space)?;
                vaddr += 0x1000;
                paddr += 0x1000;
            }
        }
        Ok(from)
    }

    #[inline]
    pub fn map_page(
        &self,
        from: VirtualAddress,
        to: PhysicalAddress,
        options: MapOptions,
        addr_space: &mut VirtualAddressSpace,
    ) -> Result<VirtualAddress, Error> {
        match options.size {
            MapSize::Size4KB => self.map_4k(from, to, options.flags, addr_space)?,
            MapSize::Size2MB => self.map_2m(from, to, options.flags, addr_space)?,
            MapSize::Size1GB => self.map_1g(from, to, options.flags, addr_space)?,
        }
        Ok(from)
    }

    fn map_4k(
        &self,
        from: VirtualAddress,
        to: PhysicalAddress,
        flags: MapFlags,
        addr_space: &mut VirtualAddressSpace,
    ) -> Result<(), Error> {
        assert!(from.is_aligned_to(0x1000));
        assert!(to.is_aligned_to(0x1000));
        let l1 = if addr_space.is_low {
            addr_space.get_table_mut()
        } else {
            let l0 = &mut addr_space.get_table_mut();
            let r = self.create_next_table(&mut l0[get_page_level_index(from, PageLevel::L0)]);
            if let Err(e) = &r {
                debug_assert!(discriminant(e) != discriminant(&Error::Memory(AlreadyMapped)));
            }
            r?
        };
        let entry = &mut l1[get_page_level_index(from, PageLevel::L1)];
        if flags.force_remap() && entry.is_present() && entry.is_block() {
            // if entry is a 1 GB block
            self.remap_block(entry, MapSize::Size1GB)?;
        }
        let l2 = self.create_next_table(entry)?;
        let entry = &mut l2[get_page_level_index(from, PageLevel::L2)];
        if flags.force_remap() && entry.is_present() && entry.is_block() {
            // if entry is a 2 MB block
            self.remap_block(entry, MapSize::Size2MB)?;
        }
        let l3 = self.create_next_table(entry)?;

        let l3_entry = &mut l3[get_page_level_index(from, PageLevel::L3)];

        if l3_entry.is_present() && !flags.force_remap() {
            return Err(Error::Memory(AlreadyMapped));
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
        flags: MapFlags,
        addr_space: &mut VirtualAddressSpace,
    ) -> Result<(), Error> {
        assert!(from.is_aligned_to(0x200000));
        assert!(to.is_aligned_to(0x200000));

        let l1 = if addr_space.is_low {
            addr_space.get_table_mut()
        } else {
            let l0 = &mut addr_space.get_table_mut();
            let r = self.create_next_table(&mut l0[get_page_level_index(from, PageLevel::L0)]);
            if let Err(e) = &r {
                debug_assert!(discriminant(e) != discriminant(&Error::Memory(AlreadyMapped)));
            }
            r?
        };
        let entry = &mut l1[get_page_level_index(from, PageLevel::L1)];
        if flags.force_remap() && entry.is_present() && entry.is_block() {
            // if entry is a 1 GB block
            self.remap_block(entry, MapSize::Size1GB)?;
        }
        let l2 = self.create_next_table(entry)?;

        let l2_entry = &mut l2[get_page_level_index(from, PageLevel::L2)];

        if l2_entry.is_present() && !flags.force_remap() {
            return Err(Error::Memory(AlreadyMapped));
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
        flags: MapFlags,
        addr_space: &mut VirtualAddressSpace,
    ) -> Result<(), Error> {
        assert!(from.is_aligned_to(0x40000000));
        assert!(to.is_aligned_to(0x40000000));

        let l1 = if addr_space.is_low {
            addr_space.get_table_mut()
        } else {
            let l0 = &mut addr_space.get_table_mut();
            let r = self.create_next_table(&mut l0[get_page_level_index(from, PageLevel::L0)]);
            if let Err(e) = &r {
                debug_assert!(discriminant(e) != discriminant(&Error::Memory(AlreadyMapped)));
            }
            r?
        };

        let l1_entry = &mut l1[get_page_level_index(from, PageLevel::L1)];

        if l1_entry.is_present() && !flags.force_remap() {
            return Err(Error::Memory(AlreadyMapped));
        }

        let l_attrib = LowerDescriptorAttributes::new()
            .with_attr_index(flags.attr_index())
            .with_shareability(flags.shareability())
            .with_EL0_access(flags.el0_access())
            .with_readonly(flags.read_only())
            .with_access_flag(1);
        let u_attrib = UpperDescriptorAttributes::new();
        *l1_entry = TableEntry::create_block_descriptor(to, l_attrib, u_attrib);

        Ok(())
    }

    fn create_next_table(
        &self,
        entry: &mut TableEntry,
    ) -> Result<&'static mut [TableEntry], Error> {
        if entry.is_present() {
            if entry.is_block() {
                return Err(Error::Memory(AlreadyMapped));
            }
            Ok(unsafe { get_table_mut(entry.addr().to_virt().as_ptr()) })
        } else {
            unsafe {
                let page = self
                    .page_allocator
                    .alloc(1)
                    .ok_or(Error::Memory(OutOfPhysicalMemory))?;
                ptr::write_bytes(page.to_virt().as_ptr::<u8>(), 0, PAGE_SIZE); // clear page

                *entry = TableEntry::create_table_descriptor(page);
                Ok(get_table_mut(entry.addr().to_virt().as_ptr()))
            }
        }
    }

    // remap a block with 512 block of the lower size
    fn remap_block(&self, entry: &mut TableEntry, entry_size: MapSize) -> Result<(), Error> {
        assert!(entry.is_present());
        assert!(entry.is_block());
        assert!(entry_size != MapSize::Size4KB);
        let table = self
            .page_allocator
            .alloc(1)
            .ok_or(Error::Memory(OutOfPhysicalMemory))?;
        unsafe {
            ptr::write_bytes(table.to_virt().as_ptr::<u8>(), 0, PAGE_SIZE);
        }
        let l2 = unsafe { get_table_mut(table.to_virt().as_ptr()) };
        let (l_attrib, u_attrib) = unsafe {
            (
                entry.block_descriptor.lower_attributes(),
                entry.block_descriptor.upper_attributes(),
            )
        };
        let mut to = entry.addr();
        for entry in l2 {
            if entry_size == MapSize::Size1GB {
                *entry = TableEntry::create_block_descriptor(to, l_attrib, u_attrib);
                to += 2 * 1024 * 1024; // 2 MB
            } else {
                *entry = TableEntry::create_page_descriptor(to, l_attrib, u_attrib);
                to += 0x1000; // 4 KB
            }
        }

        *entry = TableEntry::create_table_descriptor(table);

        Ok(())
    }

    #[inline]
    pub fn unmap(
        &self,
        addr: VirtualAddress,
        size: MapSize,
        addr_space: &mut VirtualAddressSpace,
    ) -> Result<PhysicalAddress, Error> {
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
    ) -> Result<PhysicalAddress, Error> {
        let l1 = if addr_space.is_low {
            addr_space.get_table_mut()
        } else {
            let l0 = addr_space.get_table_mut();
            self.get_table(&l0[get_page_level_index(addr, PageLevel::L0)])?
        };
        let l2 = self.get_table(&l1[get_page_level_index(addr, PageLevel::L1)])?;
        let l3 = self.get_table(&l2[get_page_level_index(addr, PageLevel::L2)])?;
        let entry = &mut l3[get_page_level_index(addr, PageLevel::L3)];

        if !entry.is_present() {
            return Err(Error::Memory(NotMapped));
        }

        let r_addr = entry.unmap();
        invalidate_addr(addr);

        Ok(r_addr)
    }

    fn unmap_2m(
        &self,
        addr: VirtualAddress,
        addr_space: &mut VirtualAddressSpace,
    ) -> Result<PhysicalAddress, Error> {
        let l1 = if addr_space.is_low {
            addr_space.get_table_mut()
        } else {
            let l0 = addr_space.get_table_mut();
            self.get_table(&l0[get_page_level_index(addr, PageLevel::L0)])?
        };
        let l2 = self.get_table(&l1[get_page_level_index(addr, PageLevel::L1)])?;
        let entry = &mut l2[get_page_level_index(addr, PageLevel::L2)];

        if !entry.is_present() {
            return Err(Error::Memory(NotMapped));
        }

        let r_addr = entry.unmap();
        invalidate_addr(addr);

        Ok(r_addr)
    }

    fn unmap_1g(
        &self,
        addr: VirtualAddress,
        addr_space: &mut VirtualAddressSpace,
    ) -> Result<PhysicalAddress, Error> {
        let l1 = if addr_space.is_low {
            addr_space.get_table_mut()
        } else {
            let l0 = addr_space.get_table_mut();
            self.get_table(&l0[get_page_level_index(addr, PageLevel::L0)])?
        };
        let entry = &mut l1[get_page_level_index(addr, PageLevel::L1)];

        if !entry.is_present() {
            return Err(Error::Memory(NotMapped));
        }

        let r_addr = entry.unmap();
        invalidate_addr(addr);

        Ok(r_addr)
    }

    fn get_table(&self, entry: &TableEntry) -> Result<&'static mut [TableEntry], Error> {
        if !entry.is_present() {
            return Err(Error::Memory(NotMapped));
        }
        if entry.is_block() {
            return Err(Error::Memory(AlreadyMapped));
        }

        let addr = entry.addr();
        let addr = addr.to_virt();
        Ok(unsafe { get_table_mut(addr.as_ptr()) })
    }

    // TODO: rewrite this to allow 4KB aligned and bigger than 512GB searchs.
    /// Find free memory space of size "count * PAGE_SIZE" in `range`.
    #[allow(clippy::needless_range_loop)]
    pub fn find_free_pages(
        &self,
        count: usize,
        range: Range<VirtualAddress>,
        addr_space: &VirtualAddressSpace,
    ) -> Result<VirtualAddress, Error> {
        assert!(count > 0);
        assert!(!range.is_empty());
        assert!((count << PAGE_SHIFT) <= (range.end.addr() - range.start.addr()));
        assert!(range.start.is_aligned_to(PAGE_SIZE));
        assert!(range.end.is_aligned_to(PAGE_SIZE));
        assert!(
            range.start.is_aligned_to(1024 * 1024 * 1024)
                && range.end.is_aligned_to(1024 * 1024 * 1024),
            "Virtual memory regions must be aligned to 1 GB"
        );
        assert!(
            range.end.addr() - range.start.addr() <= 512 * 1024 * 1024 * 1024,
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

        let l1 = if addr_space.is_low {
            addr_space.get_table()
        } else {
            let l0 = addr_space.get_table();
            let r = self.get_table(&l0[l0_index]);

            match r {
                Err(Error::Memory(AlreadyMapped)) => return Err(Error::Memory(OutOfVirtualSpace)),
                Err(Error::Memory(NotMapped)) => return Ok(range.start),
                Err(e) => return Err(e),
                Ok(table) => table,
            }
        };

        let page_off = if addr_space.is_low {
            0
        } else {
            HIGH_ADDR_SPACE_RANGE.start.addr() / PAGE_SIZE
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
                    let l2 = unsafe { get_table(l1_entry.addr().to_virt().as_ptr()) };
                    for l2_index in 0..ENTRIES_IN_TABLE {
                        let l2_entry = &l2[l2_index];
                        if l2_entry.is_present() {
                            if l2_entry.is_block() {
                                found_pages = 0;
                                start_page = None;
                            } else {
                                let l3 = unsafe { get_table(l2_entry.addr().to_virt().as_ptr()) };
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
                                            return Ok(VirtualAddress::new(
                                                start_page.unwrap() * PAGE_SIZE,
                                            ));
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
                                return Ok(VirtualAddress::new(start_page.unwrap() * PAGE_SIZE));
                            }
                        }
                    }
                }
            } else {
                start_page.get_or_insert((l0_index * 512 + l1_index) * 512 * 512 + page_off);
                found_pages += 512 * 512;
                if found_pages >= count {
                    return Ok(VirtualAddress::new(start_page.unwrap() * PAGE_SIZE));
                }
            }
        }

        Err(Error::Memory(OutOfVirtualSpace))
    }
}
