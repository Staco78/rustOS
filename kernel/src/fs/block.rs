use core::{
    fmt::Debug,
    marker::PhantomData,
    mem::{self, size_of},
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use alloc::{boxed::Box, string::String};
use hashbrown::HashMap;
use spin::lock_api::RwLock;

use crate::{
    create_fs_node,
    error::Error,
    fs::node::{Block, FsNodeInfos},
    utils::{buffer::Buffer, smart_ptr::SmartPtrResizableBuff},
};

use super::{
    devfs,
    node::{File, FsNode, FsNodeRef},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockIndex(pub usize);

static BLOCK_DEVICES: SmartPtrResizableBuff<FsNode<BlockDevice>> = SmartPtrResizableBuff::new();

pub fn register_device(device: Box<dyn BlockDev>) {
    let device = BlockDevice::new(device);
    let name = device.dev.infos().name.clone();
    let node = create_fs_node!(
        device,
        FsNodeInfos {
            size: device.size()
        },
        block: dyn Block,
        file: dyn File
    );
    let device = BLOCK_DEVICES.insert(node);
    devfs::add_device(name, FsNodeRef::new(device));
}

#[derive(Debug)]
pub struct BlockDevice {
    dev: Box<dyn BlockDev>,
    block_size: usize,
    cache: RwLock<HashMap<BlockIndex, CachedBlock>>,
}

unsafe impl Block for BlockDevice {
    fn block_size(&self) -> usize {
        self.block_size
    }

    fn get_block(&self, block: BlockIndex) -> Result<BlockRef, Error> {
        let cache = self.cache.read();
        if let Some(block) = self.get_block_ref(&cache, block) {
            Ok(block)
        } else {
            drop(cache);
            let mut cache = self.cache.write();
            if let Some(block) = self.get_block_ref(&cache, block) {
                return Ok(block);
            }
            let cached_block = self.read_block(block)?;
            let block_ref = cached_block.get_ref(self, block);
            let r = cache.insert(block, cached_block);
            debug_assert!(r.is_none());
            Ok(block_ref)
        }
    }

    fn get_block_mut(&self, block: BlockIndex) -> Result<BlockMut, Error> {
        let cache = self.cache.read();
        if let Some(block) = self.get_block_ref_mut(&cache, block) {
            Ok(block)
        } else {
            drop(cache);
            let mut cache = self.cache.write();
            if let Some(block) = self.get_block_ref_mut(&cache, block) {
                return Ok(block);
            }
            let cached_block = self.read_block(block)?;
            let block_mut = cached_block.get_mut(self, block);
            let r = cache.insert(block, cached_block);
            debug_assert!(r.is_none());
            Ok(block_mut)
        }
    }

    fn write_block(&self, block: BlockIndex, buff: &Buffer) -> Result<(), Error> {
        assert_eq!(buff.len(), self.block_size);
        let cache = self.cache.read();
        if let Some(mut block) = self.get_block_ref_mut(&cache, block) {
            block.copy_from_slice(buff);
            Ok(())
        } else {
            drop(cache);
            let mut cache = self.cache.write();
            if let Some(mut block) = self.get_block_ref_mut(&cache, block) {
                block.copy_from_slice(buff);
                return Ok(());
            }
            let mut buffer = Buffer::new_boxed(self.block_size);
            buffer.write(0, buff);
            let cached_block = CachedBlock::new(CacheState::Modified, buffer);
            cache.insert(block, cached_block);
            Ok(())
        }
    }
}

impl BlockDevice {
    pub fn new(dev: Box<dyn BlockDev>) -> Self {
        let block_size = dev.infos().block_size;
        let cache = RwLock::new(HashMap::new());
        Self {
            dev,
            block_size,
            cache,
        }
    }

    pub fn size(&self) -> usize {
        let infos = self.dev.infos();
        infos.block_count * infos.block_size
    }

    pub fn flush(&self) -> Result<(), Error> {
        todo!()
    }

    fn get_block_ref(
        &self,
        cache: &HashMap<BlockIndex, CachedBlock>,
        block: BlockIndex,
    ) -> Option<BlockRef> {
        let cached_block = cache.get(&block);
        cached_block.map(|cached_block| cached_block.get_ref(self, block))
    }

    fn get_block_ref_mut(
        &self,
        cache: &HashMap<BlockIndex, CachedBlock>,
        block: BlockIndex,
    ) -> Option<BlockMut> {
        let cached_block = cache.get(&block);
        cached_block.map(|cached_block| cached_block.get_mut(self, block))
    }

    fn read_block(&self, block: BlockIndex) -> Result<CachedBlock, Error> {
        // TODO: bypass heap allocator and use instead the vmm
        let mut buff = Buffer::new_boxed(self.block_size);
        self.dev.read(block, &mut buff)?;

        let block = CachedBlock::new(CacheState::Clean, buff);

        Ok(block)
    }
}

#[derive(Debug)]
struct CachedBlock {
    inner: RwLock<CachedBlockInner>,
}

#[derive(Debug)]
struct CachedBlockInner {
    state: CacheState,
    data: NonNull<u8>,
}

unsafe impl Send for CachedBlockInner {}
unsafe impl Sync for CachedBlockInner {}

impl CachedBlock {
    fn new(state: CacheState, buff: Box<Buffer>) -> Self {
        let ptr = buff.as_ptr() as *mut _;
        mem::forget(buff);
        Self {
            inner: RwLock::new(CachedBlockInner {
                state,
                data: NonNull::new(ptr).unwrap(),
            }),
        }
    }

    fn get_ref<'a>(&self, device: &'a BlockDevice, block: BlockIndex) -> BlockRef<'a> {
        let guard = self.inner.read();
        let ptr = guard.data;
        mem::forget(guard);
        let data = NonNull::slice_from_raw_parts(ptr, device.block_size);
        BlockRef {
            block,
            data,
            device,
        }
    }

    fn get_mut<'a>(&self, device: &'a BlockDevice, block: BlockIndex) -> BlockMut<'a> {
        let guard = self.inner.write();
        let ptr = guard.data;
        mem::forget(guard);
        let data = NonNull::slice_from_raw_parts(ptr, device.block_size);
        BlockMut {
            block,
            data,
            device,
        }
    }
}

#[derive(Debug)]
enum CacheState {
    Clean,
    Modified,
}

#[derive(Debug)]
pub struct BlockRef<'a> {
    block: BlockIndex,
    data: NonNull<[u8]>,
    device: &'a BlockDevice,
}

unsafe impl<'a> Send for BlockRef<'a> {}
unsafe impl<'a> Sync for BlockRef<'a> {}

impl<'a> Deref for BlockRef<'a> {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        unsafe { self.data.as_ref() }
    }
}

impl<'a> Drop for BlockRef<'a> {
    fn drop(&mut self) {
        let cache = self.device.cache.read();
        let block = cache.get(&self.block).unwrap();
        // Safety: Each `BlockRef` is created just after `read()`ing the `RwLock` and forgetting the guard.
        unsafe { block.inner.force_unlock_read() };
    }
}

#[derive(Debug)]
pub struct BlockMut<'a> {
    block: BlockIndex,
    data: NonNull<[u8]>,
    device: &'a BlockDevice,
}

unsafe impl<'a> Send for BlockMut<'a> {}
unsafe impl<'a> Sync for BlockMut<'a> {}

impl<'a> BlockMut<'a> {
    pub fn mark_dirty(&self) {
        let cache = self.device.cache.read();
        let block = cache.get(&self.block).unwrap();
        // Safety: Each `BlockMut` is created just after `write()`ing the `RwLock` and forgetting the guard so we know there are no other reader or writer.
        let data = unsafe { &mut *block.inner.data_ptr() };
        data.state = CacheState::Modified;
    }
}

impl<'a> Deref for BlockMut<'a> {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        unsafe { self.data.as_ref() }
    }
}

impl<'a> DerefMut for BlockMut<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.data.as_mut() }
    }
}

impl<'a> Drop for BlockMut<'a> {
    fn drop(&mut self) {
        let cache = self.device.cache.read();
        let block = cache.get(&self.block).unwrap();
        // Safety: Each `BlockMut` is created just after `write()`ing the `RwLock` and forgetting the guard so we know there are no other reader or writer.
        let data = unsafe { &mut *block.inner.data_ptr() };
        data.state = CacheState::Modified;
        // Safety: Same as above.
        unsafe { block.inner.force_unlock_write() };
    }
}

#[derive(Debug, Clone)]
pub struct BlockDevInfos {
    pub block_size: usize,
    pub block_count: usize,
    pub name: String,
}

pub trait BlockDev: Debug + Send + Sync {
    fn infos(&self) -> &BlockDevInfos;

    fn read(&self, block: BlockIndex, buff: &mut Buffer) -> Result<(), Error>;
    fn write(&self, block: BlockIndex, buff: &Buffer) -> Result<(), Error>;
}

unsafe impl File for BlockDevice {
    fn read(&self, offset: usize, buff: &mut Buffer) -> Result<usize, Error> {
        let block_size = self.block_size;
        let mut offset = offset;
        let mut buff_offset = 0;
        while buff_offset < buff.len() {
            let block_index = offset / block_size;
            let in_block_offset = offset % block_size;
            let block = self.get_block(BlockIndex(block_index))?;
            // TODO: return early if EOF
            let write_count = buff.write(
                buff_offset,
                &block[in_block_offset..block_size.min(in_block_offset + buff.len() - buff_offset)],
            );
            offset += write_count;
            buff_offset += write_count;
        }

        debug_assert_eq!(buff_offset, buff.len());

        Ok(buff_offset)
    }

    fn write(&self, offset: usize, buff: &Buffer) -> Result<usize, Error> {
        let block_size = self.block_size;
        let mut offset = offset;
        let mut buff_offset = 0;
        while buff_offset < buff.len() {
            let block_index = offset / block_size;
            let block = BlockIndex(block_index);
            let in_block_offset = offset % block_index;
            let data = buff.slice(
                buff_offset
                    ..buff_offset + (buff.len() - buff_offset).min(block_size - in_block_offset),
            );
            if data.len() == block_size {
                self.write_block(block, data)?;
            } else {
                let mut block = self.get_block_mut(block)?;
                let block_buff = Buffer::from_init_slice_mut(block.deref_mut());
                block_buff.write(in_block_offset, data);
            }
            offset += data.len();
            buff_offset += data.len();
        }

        debug_assert_eq!(buff_offset, buff.len());

        Ok(buff_offset)
    }
}

#[derive(Debug)]
pub struct TypedBlockRef<'a, T> {
    inner: BlockRef<'a>,
    offset: usize,
    phantom: PhantomData<&'a T>,
}

impl<'a, T> TypedBlockRef<'a, T> {
    /// # Safety
    /// - same as `mem::transmute<[u8; n], T>`
    /// - offset + size_of::<T>() should be <= as the block size
    unsafe fn new(inner: BlockRef<'a>, offset: usize) -> Self {
        debug_assert!(offset + size_of::<T>() <= inner.data.len());
        assert!(inner.data.as_ptr().cast::<T>().is_aligned());
        Self {
            inner,
            offset,
            phantom: PhantomData,
        }
    }
}

impl<'a, T> Deref for TypedBlockRef<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        let data = self.inner.deref();
        unsafe {
            let ptr = data.as_ptr().add(self.offset) as *const T;
            &*ptr
        }
    }
}

#[derive(Debug)]
pub struct TypedBlockMut<'a, T> {
    inner: BlockMut<'a>,
    offset: usize,
    phantom: PhantomData<&'a T>,
}

impl<'a, T> TypedBlockMut<'a, T> {
    /// # Safety
    /// - same as `mem::transmute<[u8; n], T>`
    /// - offset + size_of::<T>() should be <= as the block size
    unsafe fn new(inner: BlockMut<'a>, offset: usize) -> Self {
        debug_assert!(offset + size_of::<T>() <= inner.data.len());
        assert!(inner.data.as_ptr().cast::<T>().is_aligned());
        Self {
            inner,
            offset,
            phantom: PhantomData,
        }
    }
}

impl<'a, T> Deref for TypedBlockMut<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        let data = self.inner.deref();
        unsafe {
            let ptr = data.as_ptr().add(self.offset) as *const T;
            &*ptr
        }
    }
}

impl<'a, T> DerefMut for TypedBlockMut<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let data = self.inner.deref_mut();
        unsafe {
            let ptr = data.as_mut_ptr().add(self.offset) as *mut T;
            &mut *ptr
        }
    }
}

/// Get a block that deref as [T].
///
/// # Safety
/// [T] should be safe to transmute from [\[u8\]].
///
/// # Panic
/// Panic if `offset + size_of::<T>() > device.block_size()`.
pub unsafe fn get_typed_block<T>(
    device: &dyn Block,
    block: BlockIndex,
    offset: usize,
) -> Result<TypedBlockRef<T>, Error> {
    assert!(offset + size_of::<T>() <= device.block_size());
    let block = device.get_block(block)?;
    let typed = unsafe { TypedBlockRef::new(block, offset) };
    Ok(typed)
}

/// The mut version of `get_typed_block`.
/// Same safety and panics as `get_typed_block`.
pub unsafe fn get_typed_block_mut<T>(
    device: &dyn Block,
    block: BlockIndex,
    offset: usize,
) -> Result<TypedBlockMut<T>, Error> {
    assert!(offset + size_of::<T>() <= device.block_size());
    let block = device.get_block_mut(block)?;
    let typed = unsafe { TypedBlockMut::new(block, offset) };
    Ok(typed)
}
