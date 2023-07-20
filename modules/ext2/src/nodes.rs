use alloc::{
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use kernel::{
    create_fs_node,
    error::Error,
    fs::node::{Directory, File, FsNode, FsNodeInfos, FsNodeRef},
    utils::buffer::Buffer,
};

use crate::{filesystem::FileSystem, structs::InodeRef};

#[derive(Debug)]
pub struct FileNode<'a> {
    fs: Arc<FileSystem<'a>>,
    inode: InodeRef,
}

impl<'a> FileNode<'a> {
    #[inline(always)]
    pub fn new(fs: Arc<FileSystem<'a>>, inode: InodeRef) -> FsNode<Self> {
        let size = inode.size();
        let file = Self { fs, inode };
        create_fs_node!(file, FsNodeInfos { size }, file: dyn File)
    }
}

unsafe impl<'a> File for FileNode<'a> {
    fn read(&self, offset: usize, buff: &mut Buffer) -> Result<usize, Error> {
        let block_size = self.fs.superblock.block_size();
        let size = buff.len();

        let start_block = offset / block_size;
        let start_block_off = offset % block_size;
        let end_block = (offset + size) / block_size;
        let end_block_off = (offset + size) % block_size;

        let mut offset = 0;
        for block_idx in start_block..=end_block {
            if block_idx == start_block {
                let buff = buff.slice_mut(start_block_off..block_size.min(size));
                self.fs
                    .read_inode_block(&self.inode, block_idx, buff, start_block_off)?;
                offset += buff.len(); // Should be the same as `block_size.min(size) - start_block_off`.
            } else if block_idx == end_block {
                let buff = buff.slice_mut(offset..offset + end_block_off);
                self.fs.read_inode_block(&self.inode, block_idx, buff, 0)?;
                offset += buff.len();
            } else {
                let old_offset = offset;
                offset += block_size;
                let buff = buff.slice_mut(old_offset..offset);
                self.fs.read_inode_block(&self.inode, block_idx, buff, 0)?;
            }
        }

        debug_assert_eq!(offset, buff.len());
        Ok(buff.len())
    }
}

#[derive(Debug)]
pub struct DirNode<'a> {
    fs: Arc<FileSystem<'a>>,
    inode: InodeRef,
}

impl<'a> DirNode<'a> {
    #[inline(always)]
    pub fn new(fs: Arc<FileSystem<'a>>, inode: InodeRef) -> FsNode<Self> {
        let size = inode.size();
        let dir = Self { fs, inode };
        create_fs_node!(dir, FsNodeInfos { size }, directory: dyn Directory)
    }
}

unsafe impl<'a> Directory for DirNode<'a> {
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
