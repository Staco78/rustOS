use core::slice;

use crate::{read_cpu_reg, write_cpu_reg};
use anyhow::{anyhow, Result};
use log::trace;
use uefi::{
    boot::{self, AllocateType, MemoryType}
};

use super::{PhysicalAddress, VirtualAddress};

include!(concat!(env!("OUT_DIR"), "/mmu_pages.rs"));

mod structs {
    #![allow(dead_code)]

    use modular_bitfield::prelude::*;

    #[derive(Specifier)]
    #[bits = 2]
    pub enum Cacheability {
        NonCacheable = 0, // Normal memory, Non-cacheable
        WbWa = 1,         // Normal memory, Write-Back Write-Allocate Cacheable
        Wt = 2,           // Normal memory, Write-Through Cacheable
        WbNoWa = 3,       // Normal memory, Write-Back no Write-Allocate Cacheable
    }

    #[derive(Specifier)]
    #[bits = 2]
    pub enum Shareability {
        NonShareable = 0,
        OuterShareable = 2,
        InnerShareable = 3,
    }

    #[bitfield(bits = 64)]
    pub struct TcrRegister {
        pub t0sz: B6, // size of low memory region
        #[skip]
        reserved: B1,
        pub epd0: B1,            // TTBR0 disabled
        pub irgn0: Cacheability, // inner cacheability
        pub orgn0: Cacheability, // outer cacheability
        pub sh0: Shareability,
        pub tg0: B2,             // Granule size: 0b00: 4KB 0b01: 64KB 0b10: 16KB
        pub t1sz: B6,            // size of high memory region
        pub a1: B1,              // 0: ASID from TTBR0 1: ASID from TTBR1
        pub epd1: B1,            // TTBR1 disabled
        pub irgn1: Cacheability, // inner cacheability
        pub orgn1: Cacheability, // outer cacheability
        pub sh1: Shareability,
        pub tg1: B2, // Granule size: 0b00: 16KB 0b10: 4KB 0b11: 64KB
        pub ips: B3, // Intermediate physical address size (0b101 for 48 bits)
        #[skip]
        reserved2: B1,
        pub asid_size: B1, // 0: 8 bit 1: 16 bit
        pub tbi0: B1,      // 0: Top byte used in address calculation 1: top byte ignored
        pub tbi1: B1,      // same
        #[skip]
        reserved3: B25,
    }

    #[bitfield(bits = 12)]
    #[derive(Clone, Copy, Specifier)]
    pub struct UpperDescriptorAttributes {
        pub contigous: bool,
        #[allow(non_snake_case)]
        pub PXN: bool, // execute never at EL1
        #[allow(non_snake_case)]
        pub UXN: bool, // execute never at EL0
        #[skip]
        reserved: B4,
        #[skip]
        ignored: B5,
    }

    #[bitfield(bits = 10)]
    #[derive(Clone, Copy, Specifier)]
    pub struct LowerDescriptorAttributes {
        pub attr_index: B3, // MAIR index
        #[allow(non_snake_case)]
        non_secure: B1,
        #[allow(non_snake_case)]
        pub EL0_access: B1, // 0: no access in EL0 1: same access in EL0 and EL1 (defined by read only bit)
        pub readonly: B1,
        pub shareability: B2, // 00: non shareable 01: reserved 10: outer shareable 11: inner shareable
        pub access_flag: B1,
        pub non_global: B1,
    }

    #[bitfield(bits = 64)]
    #[derive(Clone, Copy)]
    pub struct BlockDescriptor {
        pub present: bool,
        pub block_or_table: B1, // should be 0 for block
        pub lower_attributes: LowerDescriptorAttributes,
        pub address: B36,
        #[skip]
        reserved: B4,
        pub upper_attributes: UpperDescriptorAttributes,
    }

    #[bitfield(bits = 64)]
    #[derive(Clone, Copy)]
    pub struct TableDescriptor {
        pub present: bool,
        pub block_or_table: B1, // should be 1
        #[skip]
        ignored: B10,
        pub address: B36,
        #[skip]
        reserved: B4,
        #[skip]
        ignored2: B7,

        // overrides
        #[allow(non_snake_case)]
        pub PXN: B1,
        #[allow(non_snake_case)]
        pub UXN: B1,
        #[allow(non_snake_case)]
        pub EL0_access: B1,
        pub readonly: B1,
        pub non_secure: B1,
    }
}
use structs::*;

#[derive(Clone, Copy)]
union TableEntry {
    bits: u64,
    block_descriptor: BlockDescriptor,
    table_descriptor: TableDescriptor,
}

#[repr(align(4096))]
struct Table([TableEntry; 512]);

impl TableEntry {
    fn create_table_descriptor(address: VirtualAddress) -> Self {
        debug_assert!(
            address.0 & 0xFFF == 0,
            "Table address must be aligned to 4KB"
        );
        let table = TableDescriptor::new()
            .with_present(true)
            .with_block_or_table(1)
            .with_address((address.0 & 0xFFFF_FFFF_FFFF) >> 12);
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
            .with_address((address & 0xFFFF_FFFF_FFFF) >> 12)
            .with_upper_attributes(u_attrib);
        TableEntry {
            block_descriptor: bd,
        }
    }
}

pub fn init() {
    unsafe {
        TABLE_HIGH.0[0] = TableEntry::create_table_descriptor(VirtualAddress(
            &raw const TABLE_HIGH_L1_0 as *const _ as u64,
        ));
        TABLE_HIGH.0[511] = TableEntry::create_table_descriptor(VirtualAddress(
            &raw const TABLE_HIGH_L1_511 as *const _ as u64,
        ));
    }

    write_cpu_reg!("TTBR0_EL1", &raw const TABLE_LOW as *const _ as u64);
    write_cpu_reg!("TTBR1_EL1", &raw const TABLE_HIGH as *const _ as u64);

    // This equates to:
    // 0 = b01000100 = Normal, Inner/Outer Non-Cacheable
    // 1 = b11111111 = Normal, Inner/Outer WB/WA/RA
    // 2 = b00000000 = Device-nGnRnE
    write_cpu_reg!("MAIR_EL1", 0x000000000000FF44);

    let tcr = TcrRegister::new()
        .with_t0sz(25)
        .with_irgn0(Cacheability::WbWa)
        .with_orgn0(Cacheability::WbWa)
        .with_sh0(Shareability::InnerShareable)
        .with_tg0(0)
        .with_t1sz(16)
        .with_a1(0)
        .with_irgn1(Cacheability::WbWa)
        .with_orgn1(Cacheability::WbWa)
        .with_sh1(Shareability::InnerShareable)
        .with_tg1(0b10)
        .with_ips(0b101);

    write_cpu_reg!("TCR_EL1", u64::from_le_bytes(tcr.into_bytes()));

    let mut sctlr = read_cpu_reg!("SCTLR_EL1");
    sctlr |= 1; // enable MMU
    write_cpu_reg!("SCTLR_EL1", sctlr);
}

pub fn map_page(
    from: VirtualAddress,
    to: PhysicalAddress,
) -> Result<()> {
    assert!(
        from.0 >= 0xFFFF_0000_0000_0000u64,
        "Currently only support mapping page in the upper part of the address space"
    );
    trace!("Map {:#016X?} => {:#016X?}", from.0, to);
    let l0: &mut [TableEntry; 512] = unsafe { &mut *&raw mut TABLE_HIGH.0 };
    let l0_entry = &mut l0[from.get_l0_index()];
    let l0_entry_desc = unsafe { &mut l0_entry.table_descriptor };
    if l0_entry_desc.present() && l0_entry_desc.block_or_table() == 0 {
        return Err(anyhow!("Page already mapped"));
    }
    let l1: &mut [TableEntry] = if l0_entry_desc.present() {
        unsafe {
            slice::from_raw_parts_mut((l0_entry_desc.address() << 12) as *mut TableEntry, 512)
        }
    } else {
        let page = boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1)
            .unwrap();
        *l0_entry = TableEntry::create_table_descriptor(VirtualAddress(page.addr().get() as u64));
        unsafe { slice::from_raw_parts_mut(page.as_ptr() as *mut TableEntry, 512) }
    };

    let l1_entry = &mut l1[from.get_l1_index()];
    let l1_entry_desc = unsafe { &l1_entry.table_descriptor };
    if l1_entry_desc.present() && l1_entry_desc.block_or_table() == 0 {
        return Err(anyhow!("Page already mapped"));
    }
    let l2: &mut [TableEntry] = if l1_entry_desc.present() {
        unsafe {
            slice::from_raw_parts_mut((l1_entry_desc.address() << 12) as *mut TableEntry, 512)
        }
    } else {
        let page = boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1)
            .unwrap();
        *l1_entry = TableEntry::create_table_descriptor(VirtualAddress(page.addr().get() as u64));
        unsafe { slice::from_raw_parts_mut(page.as_ptr() as *mut TableEntry, 512) }
    };

    let l2_entry = &mut l2[from.get_l2_index()];
    let l2_entry_desc = unsafe { &mut l2_entry.table_descriptor };
    if l2_entry_desc.present() && l2_entry_desc.block_or_table() == 0 {
        return Err(anyhow!("Page already mapped"));
    }
    let l3: &mut [TableEntry] = if l2_entry_desc.present() {
        unsafe {
            slice::from_raw_parts_mut((l2_entry_desc.address() << 12) as *mut TableEntry, 512)
        }
    } else {
        let page = boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1)
            .unwrap();
        *l2_entry = TableEntry::create_table_descriptor(VirtualAddress(page.addr().get() as u64));
        unsafe { slice::from_raw_parts_mut(page.as_ptr() as *mut TableEntry, 512) }
    };

    let l3_entry = &mut l3[from.get_l3_index()];
    let l3_entry_desc = unsafe { &mut l3_entry.table_descriptor };
    if l3_entry_desc.present() {
        return Err(anyhow!("Warn: Page already mapped"));
    }

    let l_attrib = LowerDescriptorAttributes::new()
        .with_attr_index(1)
        .with_shareability(0b11)
        .with_access_flag(1);
    let u_attrib = UpperDescriptorAttributes::new();
    *l3_entry = TableEntry::create_page_descriptor(to, l_attrib, u_attrib);

    Ok(())
}
