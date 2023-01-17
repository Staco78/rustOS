use core::{mem::size_of, ops::Range};

use super::VirtualAddress;

pub const PAGE_SHIFT: usize = 12;
pub const PAGE_SIZE: usize = 1 << PAGE_SHIFT; // 4 KB
pub const ENTRIES_IN_TABLE: usize = PAGE_SIZE / size_of::<usize>();

pub const INVALID_VIRT_ADDRESS_RANGE: Range<usize> = 0x8000000000..0xFFFF000000000000;

pub const LOW_ADDR_SPACE_RANGE: Range<VirtualAddress> =
    VirtualAddress::new(0)..VirtualAddress::new(INVALID_VIRT_ADDRESS_RANGE.start); // 512 GB
pub const HIGH_ADDR_SPACE_RANGE: Range<VirtualAddress> =
    VirtualAddress::new(INVALID_VIRT_ADDRESS_RANGE.end)..VirtualAddress::new(usize::MAX); // 256 TB

/// Where the low physical addr space is mapped
pub const PHYSICAL_LINEAR_MAPPING_RANGE: Range<VirtualAddress> =
    VirtualAddress::new(0xFFFF_0000_0000_0000)..VirtualAddress::new(0xFFFF_0080_0000_0000); // 512 GB
pub const KERNEL_HEAP_RANGE: Range<VirtualAddress> =
    VirtualAddress::new(0xFFFF_0080_0000_0000)..VirtualAddress::new(0xFFFF_0100_0000_0000); // 512 GB
                                                                                            // pub const MODULES_SPACE_RANGE: Range<VirtualAddress> =
                                                                                            // VirtualAddress::new(0xFFFF_0100_0000_0000)..VirtualAddress::new(0xFFFF_0104_0000_0000); // 16 GB
pub const MODULES_SPACE_RANGE: Range<VirtualAddress> =
    VirtualAddress::new(0xFFFF_FFFF_0000_0000)..VirtualAddress::new(0xFFFF_FFFF_FFFF_FFFF);
pub const USER_SPACE_RANGE: Range<VirtualAddress> =
    VirtualAddress::new(0x40000000)..VirtualAddress::new(0x8000000000);
