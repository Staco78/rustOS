use core::{assert_matches::debug_assert_matches, ops::Deref};

use alloc::{
    boxed::Box,
    sync::{Arc, Weak},
};
use kernel::{
    error::{Error, FsError::*},
    fs::{
        self,
        node::{File, FsNode, FsNodeRef},
    },
    utils::{
        buffer::Buffer,
        smart_ptr::{SmartPtrDeref, SmartPtrResizableBuff},
    },
};
use log::{info, warn};

use crate::{
    consts::{FILE_NODE_BUFF_SIZE, ROOT_INODE, SIGNATURE},
    icache::InodeCache,
    nodes::{DirNode, FileNode},
    structs::{BlockGroupDescriptor, DirEntry, Inode, InodeRef, SuperBlock, Type},
};

pub type BlockIndex = usize;
pub type InodeIndex = usize;

#[derive(Debug)]
pub struct FileSystem<'a> {
    device: SmartPtrDeref<'a, FsNode<()>, dyn File>,
    pub superblock: SuperBlock,
    block_group_table: Box<[BlockGroupDescriptor]>,
    weak: Weak<Self>,
    files: SmartPtrResizableBuff<FsNode<FileNode<'a>>, FILE_NODE_BUFF_SIZE>,
    dirs: SmartPtrResizableBuff<FsNode<DirNode<'a>>, FILE_NODE_BUFF_SIZE>,
    inode_cache: InodeCache,
}

impl<'a> FileSystem<'a> {
    pub fn new(device: FsNodeRef) -> Result<Arc<Self>, Error> {
        let device = FsNodeRef::clone(&device)
            .into_file()
            .ok_or(Error::Fs(NotAFile))?;
        let superblock = Self::read_superblock(&*device)?;
        let block_group_table = Self::read_block_group_descriptor_table(&*device, &superblock)?;
        let s = Arc::new_cyclic(|weak| Self {
            device,
            superblock,
            block_group_table,
            weak: weak.clone(),
            files: SmartPtrResizableBuff::new(),
            dirs: SmartPtrResizableBuff::new(),
            inode_cache: InodeCache::new(),
        });
        Ok(s)
    }

    pub fn get_root_node(&self) -> Result<FsNodeRef, Error> {
        let inode = self.read_inode(ROOT_INODE)?;
        self.file_from_inode(inode)
    }

    fn read_superblock(device: &dyn File) -> Result<SuperBlock, Error> {
        let superblock: SuperBlock = unsafe { fs::read_struct(device, 1024) }?;
        if superblock.signature != SIGNATURE {
            info!("Bad signature");
            return Err(Error::Fs(InvalidFS));
        }
        if superblock.required_features != 0 {
            warn!(
                "Unsupported required features ({:#b})",
                superblock.required_features
            );
            return Err(Error::Fs(InvalidFS));
        }
        Ok(superblock)
    }

    fn read_block_group_descriptor_table(
        device: &dyn File,
        superblock: &SuperBlock,
    ) -> Result<Box<[BlockGroupDescriptor]>, Error> {
        let block_groups_count = superblock
            .blocks_count
            .div_ceil(superblock.blocks_per_group) as usize;
        let table_offset = 2048_usize.next_multiple_of(superblock.block_size());
        let table: Box<[BlockGroupDescriptor]> =
            unsafe { fs::read_slice_boxed(device.deref(), table_offset, block_groups_count) }?;
        Ok(table)
    }

    pub fn read_inode(&self, index: InodeIndex) -> Result<InodeRef, Error> {
        self.inode_cache.get(index, || self.read_inode_(index))
    }

    pub fn read_inode_(&self, index: InodeIndex) -> Result<Inode, Error> {
        let block_group = (index - 1) / self.superblock.inode_per_group as usize;
        let block_group_descriptor = &self.block_group_table[block_group];
        let in_block_index = (index - 1) % self.superblock.inode_per_group as usize;
        let addr = block_group_descriptor.inode_table_addr as usize * self.superblock.block_size()
            + in_block_index * self.superblock.inode_size();
        let inode = unsafe { fs::read_struct(self.device.deref(), addr)? };
        Ok(inode)
    }

    pub fn read_block(
        &self,
        index: BlockIndex,
        buff: &mut Buffer,
        off: usize,
    ) -> Result<(), Error> {
        debug_assert!(off < self.superblock.block_size());
        let len = buff.len();
        let r = self
            .device
            .read(index * self.superblock.block_size() + off, buff)?;
        if r != len {
            return Err(Error::IoError);
        }
        Ok(())
    }

    // TODO: make reading of more than one block at a time.
    pub fn read_inode_block(
        &self,
        inode: &Inode,
        block: BlockIndex,
        buff: &mut Buffer,
        off: usize,
    ) -> Result<(), Error> {
        assert!(off < self.superblock.block_size());
        let max_block_indirect = (self.superblock.block_size() / 4) + 12;
        let max_block_double_indirect =
            (max_block_indirect - 12) * (self.superblock.block_size() / 4) + 12;
        let max_block_triple_indirect =
            (max_block_double_indirect - 12) * (self.superblock.block_size() / 4) + 12;
        if block < 12 {
            self.read_block(inode.ptrs[block] as BlockIndex, buff, off)
        } else if block < max_block_indirect {
            let block =
                self.read_indirect_block(inode.indirect_ptr as BlockIndex, block - 12, 1)?;
            self.read_block(block, buff, off)
        } else if block < max_block_double_indirect {
            let block =
                self.read_indirect_block(inode.double_indirect_ptr as BlockIndex, block - 12, 2)?;
            self.read_block(block, buff, off)
        } else if block < max_block_triple_indirect {
            let block =
                self.read_indirect_block(inode.triple_indirect_ptr as BlockIndex, block - 12, 3)?;
            self.read_block(block, buff, off)
        } else {
            // Block index out of range.
            Err(Error::Fs(InvalidFS))
        }
    }

    fn read_indirect_block(
        &self,
        index: BlockIndex,
        block: BlockIndex,
        recurs_level: usize,
    ) -> Result<BlockIndex, Error> {
        assert!(recurs_level > 0);
        assert!(recurs_level <= 3);
        let block_size = self.superblock.block_size();
        let mut buff = Buffer::new_boxed(block_size);
        self.read_block(index, &mut buff, 0)?;
        let buff = {
            let (a, b, c) = unsafe { buff.align_to::<u32>() };
            debug_assert!(a.is_empty());
            debug_assert!(c.is_empty());
            b
        };
        if recurs_level == 1 {
            Ok(buff[block] as usize)
        } else if recurs_level == 2 {
            let index = buff[block / block_size] as usize;
            let block = block % block_size;
            self.read_indirect_block(index, block, 1)
        } else if recurs_level == 3 {
            let count = (block_size / 4) * (block_size / 4);
            let index = buff[block / count] as usize;
            let block = block % count;
            self.read_indirect_block(index, block, 2)
        } else {
            unreachable!()
        }
    }

    pub fn read_dir<F>(&self, inode: &Inode, mut cb: F) -> Result<(), Error>
    where
        F: FnMut(&DirEntry) -> Result<bool, Error>,
    {
        let inode_type = Type::try_from(inode.type_and_permissions).unwrap();
        debug_assert_matches!(inode_type, Type::Dir);

        let block_size = self.superblock.block_size();

        let mut buff = Buffer::new_boxed(block_size);
        self.read_inode_block(inode, 0, &mut buff, 0)?;

        let mut offset = 0;

        // FIXME
        assert!(
            (inode.size_lower as usize) <= block_size,
            "{} > {}",
            (inode.size_lower as usize),
            block_size
        );

        while offset < inode.size_lower as usize {
            let entry = unsafe {
                let ptr = buff[offset..].as_ptr() as *const DirEntry;
                ptr.as_ref().unwrap_unchecked()
            };

            offset += entry.entry_size as usize;

            let r = cb(entry)?;
            if !r {
                return Ok(());
            }
        }

        Ok(())
    }

    #[inline]
    pub fn file_from_dir_entry(&self, dir_entry: &DirEntry) -> Result<FsNodeRef, Error> {
        let inode = self.read_inode(dir_entry.inode as InodeIndex)?;
        self.file_from_inode(inode)
    }

    pub fn file_from_inode(&self, inode: InodeRef) -> Result<FsNodeRef, Error> {
        let inode_type =
            Type::try_from(inode.type_and_permissions).map_err(|_| Error::Fs(InvalidFS))?;
        match inode_type {
            Type::File => {
                let file = FileNode::new(self.weak.upgrade().expect("Arc destroyed"), inode);
                let node = self.files.insert(file);
                let node = FsNodeRef::new(node);
                Ok(node)
            }
            Type::Dir => {
                let dir = DirNode::new(self.weak.upgrade().expect("Arc destroyed"), inode);
                let node = self.dirs.insert(dir);
                let node = FsNodeRef::new(node);
                Ok(node)
            }
            _ => unimplemented!(),
        }
    }
}
