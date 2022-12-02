use core::{mem::size_of, ops::Range};

pub const PAGE_SHIFT: usize = 12;
pub const PAGE_SIZE: usize = 1 << PAGE_SHIFT; // 4 KB
pub const ENTRIES_IN_TABLE: usize = PAGE_SIZE / size_of::<usize>();

pub const USER_VIRT_SPACE_RANGE: Range<usize> = 0..0x8000000000; // 512 GB
pub const KERNEL_VIRT_SPACE_RANGE: Range<usize> = 0xFFFF000000000000..0xFFFFFFFFFFFFFFFF; // 256 TB
pub const PHYSICAL_LINEAR_MAPPING_RANGE: Range<usize> = 0xFFFF_0000_0000_0000..0xFFFF_0080_0000_0000; // 512 GB
pub const KERNEL_HEAP_RANGE: Range<usize> = 0xFFFF_0080_0000_0000..0xFFFF_0100_0000_0000; // 512 GB
pub const MODULES_SPACE_RANGE: Range<usize> = 0xFFFF_0100_0000_0000..0xFFFF_0104_0000_0000; // 16 GB
pub const USER_SPACE_RANGE: Range<usize> = 0x40000000..0x8000000000;