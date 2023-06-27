#![allow(unused)]

pub const FILE_NODE_BUFF_SIZE: usize = 20;

pub const SIGNATURE: u16 = 0xEF53;

pub const OPTIONAL_FEATURE_DIR_PREALLOC: u32 = 0x1;
pub const OPTIONAL_FEATURE_AFS_SERVER: u32 = 0x2;
pub const OPTIONAL_FEATURE_JOURNAL: u32 = 0x4;
pub const OPTIONAL_FEATURE_EXTENDED_INODES: u32 = 0x8;
pub const OPTIONAL_FEATURE_RESIZE: u32 = 0x10;
pub const OPTIONAL_FEATURE_DIR_INDEX: u32 = 0x20;

pub const REQUIRED_FEATURE_COMPRESSION: u32 = 0x1;
pub const REQUIRED_FEATURE_FILETYPE: u32 = 0x2;
pub const REQUIRED_FEATURE_RECOVER: u32 = 0x4;
pub const REQUIRED_FEATURE_JOURNAL: u32 = 0x8;

pub const REQUIRED_WRITE_FEATURE_SPARSE_SUPER: u32 = 0x1;
pub const REQUIRED_WRITE_FEATURE_LARGE_FILE: u32 = 0x2;
pub const REQUIRED_WRITE_FEATURE_BTREE_DIR: u32 = 0x4;