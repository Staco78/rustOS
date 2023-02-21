use core::{mem::MaybeUninit, ops::Deref, slice};

use alloc::{
    boxed::Box,
    sync::{Arc, Weak},
    vec::Vec,
};
use kernel::{
    error::{Error, FsError::*},
    fs::{self, node::FsNodeRef},
    utils::smart_ptr::{SmartBuff, SmartPtrSizedBuff},
};
use log::{info, warn};
use spin::lock_api::RwLock;

use crate::{
    consts::{FILE_NODE_BUFF_SIZE, SIGNATURE},
    nodes::{DirNode, FileNode},
    structs::{BlockGroupDescriptor, DirEntry, Inode, SuperBlock, Type},
};

#[derive(Debug)]
pub struct FileSystem {
    device: FsNodeRef,
    pub superblock: SuperBlock,
    block_group_table: Box<[BlockGroupDescriptor]>,
    weak: Weak<Self>,
    files: RwLock<Vec<SmartPtrSizedBuff<FileNode, FILE_NODE_BUFF_SIZE>>>,
    dirs: RwLock<Vec<SmartPtrSizedBuff<DirNode, FILE_NODE_BUFF_SIZE>>>,
}

impl FileSystem {
    pub fn new(device: FsNodeRef) -> Result<Arc<Self>, Error> {
        let superblock = Self::read_superblock(&device)?;
        let block_group_table = Self::read_block_group_descriptor_table(&device, &superblock)?;
        let s = Arc::new_cyclic(|weak| Self {
            device,
            superblock,
            block_group_table,
            weak: weak.clone(),
            files: RwLock::new(Vec::new()),
            dirs: RwLock::new(Vec::new()),
        });
        let inode = s.read_inode(2)?;
        s.read_dir(&inode, |_| Ok(true))?;
        Ok(s)
    }

    pub fn get_root_node(&self) -> Result<FsNodeRef, Error> {
        let inode = self.read_inode(2)?;
        self.file_from_inode(inode)
    }

    fn read_superblock(device: &FsNodeRef) -> Result<SuperBlock, Error> {
        let superblock: SuperBlock = unsafe { fs::read_struct(device.deref(), 1024) }?;
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
        device: &FsNodeRef,
        superblock: &SuperBlock,
    ) -> Result<Box<[BlockGroupDescriptor]>, Error> {
        let block_groups_count = superblock
            .blocks_count
            .div_ceil(superblock.blocks_per_group) as usize;
        let table_offset = 2048_usize.next_multiple_of(superblock.block_size());
        let table: Vec<BlockGroupDescriptor> =
            unsafe { fs::read_struct_vec(device.deref(), table_offset, block_groups_count) }?;
        Ok(table.into_boxed_slice())
    }

    pub fn read_inode(&self, index: usize) -> Result<Inode, Error> {
        let block_group = (index - 1) / self.superblock.inode_per_group as usize;
        let block_group_descriptor = &self.block_group_table[block_group];
        let in_block_index = (index - 1) % self.superblock.inode_per_group as usize;
        let addr = block_group_descriptor.inode_table_addr as usize * self.superblock.block_size()
            + in_block_index * self.superblock.inode_size();
        let inode = unsafe { fs::read_struct(self.device.deref(), addr)? };
        Ok(inode)
    }

    pub fn read_block<'a>(
        &self,
        index: usize,
        buff: &'a mut [MaybeUninit<u8>],
        off: usize,
    ) -> Result<&'a mut [u8], Error> {
        debug_assert!(off < self.superblock.block_size());
        let len = buff.len();
        let b = self
            .device
            .read(index * self.superblock.block_size() + off, buff)?;
        if b.len() != len {
            return Err(Error::IoError);
        }
        Ok(b)
    }

    // TODO: make reading of more than one block at a time.
    pub fn read_inode_block<'a>(
        &self,
        inode: &Inode,
        block: usize,
        buff: &'a mut [MaybeUninit<u8>],
        off: usize,
    ) -> Result<&'a mut [u8], Error> {
        assert!(off < self.superblock.block_size());
        let max_block_indirect = (self.superblock.block_size() / 4) + 12;
        let max_block_double_indirect =
            (max_block_indirect - 12) * (self.superblock.block_size() / 4) + 12;
        let max_block_triple_indirect =
            (max_block_double_indirect - 12) * (self.superblock.block_size() / 4) + 12;
        if block < 12 {
            self.read_block(inode.ptrs[block] as usize, buff, off)
        } else if block < max_block_indirect {
            let block = self.read_indirect_block(inode.indirect_ptr as usize, block - 12, 1)?;
            self.read_block(block, buff, off)
        } else if block < max_block_double_indirect {
            let block =
                self.read_indirect_block(inode.double_indirect_ptr as usize, block - 12, 2)?;
            self.read_block(block, buff, off)
        } else if block < max_block_triple_indirect {
            let block =
                self.read_indirect_block(inode.triple_indirect_ptr as usize, block - 12, 3)?;
            self.read_block(block, buff, off)
        } else {
            // Block index out of range.
            Err(Error::Fs(InvalidFS))
        }
    }

    fn read_indirect_block(
        &self,
        index: usize,
        block: usize,
        recurs_level: usize,
    ) -> Result<usize, Error> {
        assert!(recurs_level > 0);
        assert!(recurs_level <= 3);
        let block_size = self.superblock.block_size();
        let mut buff = Box::new_uninit_slice(block_size);
        let buff = self.read_block(index, &mut buff, 0)?;
        let buff = unsafe { slice::from_raw_parts(buff.as_ptr() as *const u32, buff.len() / 4) };
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
        debug_assert!(matches!(inode_type, Type::Dir));

        let mut buff = Box::new_uninit_slice(self.superblock.block_size());
        let buff = self.read_inode_block(inode, 0, &mut buff, 0)? as &[u8];

        let mut offset = 0;

        // FIXME
        assert!((inode.size_lower as usize) <= self.superblock.block_size());

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
        let inode = self.read_inode(dir_entry.inode as usize)?;
        self.file_from_inode(inode)
    }

    pub fn file_from_inode(&self, inode: Inode) -> Result<FsNodeRef, Error> {
        let inode_type =
            Type::try_from(inode.type_and_permissions).map_err(|_| Error::Fs(InvalidFS))?;
        match inode_type {
            Type::File => {
                let mut file = FileNode::new(self.weak.upgrade().expect("Arc destroyed"), inode);
                for buff in self.files.read().iter() {
                    let r = buff.insert(file);
                    if let Ok((_, ptr)) = r {
                        return Ok(ptr);
                    } else if let Err(value) = r {
                        file = value;
                    } else {
                        unreachable!()
                    }
                }
                let buff = SmartPtrSizedBuff::new(true);
                let (_, ptr) = buff.insert(file).expect("Buff should be empty");
                self.files.write().push(buff);
                Ok(ptr)
            }
            Type::Dir => {
                let mut dir = DirNode::new(self.weak.upgrade().expect("Arc destroyed"), inode);
                for buff in self.dirs.read().iter() {
                    let r = buff.insert(dir);
                    if let Ok((_, ptr)) = r {
                        return Ok(ptr);
                    } else if let Err(value) = r {
                        dir = value;
                    } else {
                        unreachable!()
                    }
                }
                let buff = SmartPtrSizedBuff::new(true);
                let (_, ptr) = buff.insert(dir).expect("Buff should be empty");
                self.dirs.write().push(buff);
                Ok(ptr)
            }
            _ => unimplemented!(),
        }
    }
}
