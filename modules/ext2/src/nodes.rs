use core::mem::MaybeUninit;

use alloc::{
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use kernel::{
    error::Error,
    fs::node::{FsNode, FsNodeRef},
};

use crate::{filesystem::FileSystem, structs::Inode};

#[derive(Debug)]
pub struct FileNode {
    fs: Arc<FileSystem>,
    inode: Inode,
}

impl FileNode {
    #[inline(always)]
    pub fn new(fs: Arc<FileSystem>, inode: Inode) -> Self {
        Self { fs, inode }
    }
}

impl FsNode for FileNode {
    fn size(&self) -> Result<usize, Error> {
        let size = self.inode.size_lower as usize | (self.inode.size_upper as usize >> 32);
        Ok(size)
    }

    fn read<'a>(
        &self,
        offset: usize,
        buff: &'a mut [MaybeUninit<u8>],
    ) -> Result<&'a mut [u8], Error> {
        let block_size = self.fs.superblock.block_size();
        let size = buff.len();

        let start_block = offset / block_size;
        let start_block_off = offset % block_size;
        let end_block = (offset + size) / block_size;
        let end_block_off = (offset + size) % block_size;

        let mut offset = 0;
        for block_idx in start_block..=end_block {
            if block_idx == start_block {
                let buff = &mut buff[start_block_off..block_size.min(size)];
                let read =
                    self.fs
                        .read_inode_block(&self.inode, block_idx, buff, start_block_off)?;
                offset += read.len(); // Should be the same as `block_size - start_block_off`.
            } else if block_idx == end_block {
                let buff = &mut buff[offset..offset + end_block_off];
                self.fs.read_inode_block(&self.inode, block_idx, buff, 0)?;
            } else {
                let old_offset = offset;
                offset += block_size;
                let buff = &mut buff[old_offset..offset];
                self.fs.read_inode_block(&self.inode, block_idx, buff, 0)?;
            }
        }

        let buff = unsafe { MaybeUninit::slice_assume_init_mut(buff) };
        Ok(buff)
    }
}

#[derive(Debug)]
pub struct DirNode {
    fs: Arc<FileSystem>,
    inode: Inode,
}

impl DirNode {
    #[inline(always)]
    pub fn new(fs: Arc<FileSystem>, inode: Inode) -> Self {
        Self { fs, inode }
    }
}

impl FsNode for DirNode {
    fn find(&self, name: &str) -> Result<Option<FsNodeRef>, Error> {
        let mut file = None;
        self.fs.read_dir(&self.inode, |entry| {
            if entry.name() == name {
                let f = self.fs.file_from_dir_entry(entry)?;
                file = Some(f);
                Ok(false)
            } else {
                Ok(true)
            }
        })?;
        Ok(file)
    }

    fn list(&self) -> Result<Vec<String>, Error> {
        let mut files = Vec::new();
        self.fs.read_dir(&self.inode, |entry| {
            files.push(entry.name().to_string());
            Ok(true)
        })?;
        Ok(files)
    }
}
