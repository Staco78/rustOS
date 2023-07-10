#![allow(unused)]

use core::{ffi::CStr, mem, slice};

use static_assertions::assert_eq_size;

#[repr(C)]
#[derive(Debug)]
pub struct SuperBlock {
    pub inode_count: u32,
    pub blocks_count: u32,
    pub reserved_blocks_count: u32,
    pub free_blocks_count: u32,
    pub free_inodes_count: u32,
    pub superblock_block_idx: u32,
    pub block_size_log: u32,
    pub fragment_size_log: u32,
    pub blocks_per_group: u32,
    pub fragments_per_group: u32,
    pub inode_per_group: u32,
    pub mount_time: u32,
    pub written_time: u32,
    pub mount_count: u16,
    pub max_mount_count: u16,
    pub signature: u16,
    pub state: u16,
    pub error_behavior: u16,
    pub minor_version: u16,
    pub last_check: u32,
    pub checks_interval: u32,
    pub creator_os: u32,
    pub major_version: u32,
    pub res_uid: u16,
    pub res_gid: u16,

    // Extended fields:
    pub first_inode: u32,
    pub inode_size: u16,
    pub superblock_bg: u16,
    pub optional_features: u32,
    pub required_features: u32,
    pub write_required_features: u32,
    pub fs_id: [u8; 16],
    pub name: [u8; 16],
    pub last_mounted_path: [u8; 64],
    pub compression_algorithms: u32,
    pub prealloc_blocks: u8,
    pub prealloc_dir_blocks: u32,
    __: u16,
    pub journal_id: [u8; 16],
    pub journal_inode: u32,
    pub journal_device: u32,
    pub last_orphan: u32,
}

impl SuperBlock {
    #[inline(always)]
    pub fn block_size(&self) -> usize {
        1024 << self.block_size_log as usize
    }

    #[inline(always)]
    pub fn inode_size(&self) -> usize {
        if self.major_version >= 1 {
            self.inode_size as usize
        } else {
            128
        }
    }
}

enum FsState {
    Clean,
    Errors,
}

impl TryFrom<u16> for FsState {
    type Error = ();
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Clean),
            2 => Ok(Self::Errors),
            _ => Err(()),
        }
    }
}

enum ErrorBehavior {
    Ignore,
    RemoutRO,
    Panic,
}

impl TryFrom<u16> for ErrorBehavior {
    type Error = ();
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Ignore),
            2 => Ok(Self::RemoutRO),
            3 => Ok(Self::Panic),
            _ => Err(()),
        }
    }
}

enum CreatorOs {
    Linux,
    Hurd,
    Masix,
    FreeBSD,
    OtherBSD,
}

impl TryFrom<u16> for CreatorOs {
    type Error = ();
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Linux),
            1 => Ok(Self::Hurd),
            2 => Ok(Self::Masix),
            3 => Ok(Self::FreeBSD),
            4 => Ok(Self::OtherBSD),
            _ => Err(()),
        }
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct BlockGroupDescriptor {
    pub block_usage_bitmap_addr: u32,
    pub inode_usage_bitmap_addr: u32,
    pub inode_table_addr: u32,
    pub free_blocks_count: u16,
    pub free_inodes_count: u16,
    pub dir_count: u16,
    __: [u8; 14],
}

assert_eq_size!(BlockGroupDescriptor, [u8; 32]);

#[repr(C)]
#[derive(Debug)]
pub struct Inode {
    pub type_and_permissions: u16,
    pub uid: u16,
    pub size_lower: u32,
    pub access_time: u32,
    pub creation_time: u32,
    pub modif_time: u32,
    pub delete_time: u32,
    pub gid: u16,
    pub hard_links_count: u16,
    pub disk_sectors_count: u32,
    pub flags: u32,
    pub os_value_1: u32,
    pub ptrs: [u32; 12],
    pub indirect_ptr: u32,
    pub double_indirect_ptr: u32,
    pub triple_indirect_ptr: u32,
    pub generation_number: u32,
    pub extended_attribute_block: u32,
    pub size_upper: u32,
    pub fragment_addr: u32,
    pub os_value_2: [u8; 12],
}

assert_eq_size!(Inode, [u8; 128]);

#[derive(Debug)]
pub enum Type {
    Fifo,
    CharDev,
    Dir,
    BlockDev,
    File,
    Link,
    Socket,
}

impl TryFrom<u16> for Type {
    type Error = ();
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        let value = value & 0xF000;
        match value {
            0x1000 => Ok(Self::Fifo),
            0x2000 => Ok(Self::CharDev),
            0x4000 => Ok(Self::Dir),
            0x6000 => Ok(Self::BlockDev),
            0x8000 => Ok(Self::File),
            0xA000 => Ok(Self::Link),
            0xC000 => Ok(Self::Socket),
            _ => Err(()),
        }
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct DirEntry {
    pub inode: u32,
    pub entry_size: u16,
    pub name_len_lower: u8,
    pub name_len_upper_or_type: u8,
    name: (),
}

impl DirEntry {
    pub fn name_bytes(&self) -> &[u8] {
        let ptr = &self.name as *const _ as *const u8;
        unsafe { slice::from_raw_parts(ptr, self.name_len_lower as usize) }
    }

    pub fn name(&self) -> &str {
        let bytes = self.name_bytes();
        unsafe { core::str::from_utf8_unchecked(bytes) }
    }
}
