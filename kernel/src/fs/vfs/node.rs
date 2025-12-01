use core::{
    fmt::Debug,
    mem::{self, MaybeUninit},
    ops::Deref,
    ptr::{self, DynMetadata},
    slice,
};

use alloc::{string::String, vec::Vec};
use memoffset::offset_of;

use crate::{
    error::{Error, FsError},
    fs::block::{BlockIndex, BlockMut, BlockRef},
    utils::{buffer::Buffer, smart_ptr::SmartPtr},
};

pub type FsNodeRef = SmartPtr<FsNode<()>>;

macro_rules! into {
    ($n:ident, $a:ident, $t:ty) => {
        pub fn $n<'a>(self) -> Option<$crate::utils::smart_ptr::SmartPtrDeref<'a, FsNode<()>, $t>> {
            $crate::utils::smart_ptr::SmartPtrDeref::try_new::<_, ()>(self, |inner| {
                inner.$a().ok_or(())
            })
            .ok()
        }
    };
}

impl FsNodeRef {
    #[inline]
    pub fn new<T>(node: SmartPtr<FsNode<T>>) -> Self {
        unsafe { mem::transmute(node) }
    }

    into!(into_file, as_file, dyn File);
    into!(into_dir, as_dir, dyn Directory);
    into!(into_block, as_block, dyn Block);
}

#[derive(Debug)]
#[repr(C)]
pub struct FsNode<T> {
    pub infos: FsNodeInfos,
    vtables: FsNodeVTables,
    inner_offset: usize,
    inner: T,
}

impl<T> FsNode<T> {
    #[inline]
    pub fn new(infos: FsNodeInfos, vtables: FsNodeVTables, inner: T) -> Self {
        Self {
            infos,
            vtables,
            inner_offset: offset_of!(Self, inner),
            inner,
        }
    }
}

impl<T> Deref for FsNode<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> Drop for FsNode<T> {
    fn drop(&mut self) {
        todo!()
    }
}
#[derive(Debug)]
pub struct FsNodeInfos {
    pub size: usize,
}

#[macro_export]
macro_rules! create_fs_node {
    ($val:expr, $infos:expr, $($i:ident: $t:ty),+) => {{
        use $crate::fs::node::{FsNode, FsNodeVTables};
        use ::core::default::Default;
        let infos = $infos;
        let val = $val;
        let vtables = FsNodeVTables {
            $($i: {
                #[allow(unused_unsafe)]
                // Extend the lifetime
                let ref_ = unsafe { ::core::mem::transmute::<&$t, &$t>(&val) };
                let ptr: *const $t = ref_;
                let (_, metadata) = ptr.to_raw_parts();
                let metadata = metadata as ::core::ptr::DynMetadata<_>;
                Some(metadata)
            }),+,
            ..Default::default()
        };
        FsNode::new(infos, vtables, val)
    }};
}

macro_rules! as_inner {
    ($n:ident, $i:ident, $t:ty) => {
        pub fn $n(&self) -> Option<&$t> {
            let vtable = self.vtables.$i?;
            let self_ptr: *const Self = self;
            let inner_ptr = self_ptr.wrapping_byte_add(self.inner_offset) as *const ();
            let ptr = ptr::from_raw_parts(inner_ptr, vtable) as *const $t;
            // Safety:
            // - the ptr is valid: &self.inner is a valid reference and `vtable` is the vtable for `$t`.
            // - the lifetime of &self.inner is the same as &self
            let r = unsafe { &*ptr };
            Some(r)
        }
    };
}

impl FsNode<()> {
    unsafe fn get_infos_from_inner<T: ?Sized>(inner: &T) -> &FsNodeInfos {
        let infos_offset = offset_of!(Self, infos);
        let inner_offset = offset_of!(Self, inner);
        let ptr = inner as *const _ as *const ();
        let self_ptr = ptr.wrapping_byte_offset(infos_offset as isize - inner_offset as isize)
            as *const FsNodeInfos;

        unsafe { &*self_ptr }
    }

    as_inner!(as_file, file, dyn File);
    as_inner!(as_dir, directory, dyn Directory);
    as_inner!(as_block, block, dyn Block);
}

#[derive(Debug, Default)]
pub struct FsNodeVTables {
    pub file: Option<DynMetadata<dyn File>>,
    pub directory: Option<DynMetadata<dyn Directory>>,
    pub block: Option<DynMetadata<dyn Block>>,
}

/// Safety: any object implementing this trait should only be used inside a `FsNode`
pub unsafe trait File: Debug + Send + Sync {
    /// The size to read is the len of `buff`.
    ///
    /// `offset` is the offset from the start of the file where to start reading.
    ///
    /// Return the total of read bytes.
    fn read(&self, offset: usize, buff: &mut Buffer) -> Result<usize, Error>;

    fn read_vec(&self, offset: usize, size: usize) -> Result<Vec<u8>, Error> {
        let mut vec: Vec<u8> = Vec::with_capacity(size);
        let buff = unsafe {
            let ptr = vec.as_mut_ptr().cast::<MaybeUninit<u8>>();

            // Safety: the ptr is valid because vec allocated it for us
            slice::from_raw_parts_mut(ptr, size)
        };
        let buff = Buffer::from_slice_mut(buff);
        let len = self.read(offset, buff)?;
        debug_assert!(len <= size);

        // Safety: elements are initalized by the file read
        unsafe { vec.set_len(len) };

        Ok(vec)
    }

    fn read_to_end_vec(&self, offset: usize) -> Result<Vec<u8>, Error> {
        // Safety is ensured by the implementer of this trait.
        let infos = unsafe { FsNode::get_infos_from_inner(self) };
        let size = infos.size;
        if offset >= size {
            return Ok(Vec::new());
        }

        let to_read = size - offset;
        self.read_vec(offset, to_read)
    }

    #[inline]
    fn read_to_end_string(&self) -> Result<String, Error> {
        String::from_utf8(self.read_to_end_vec(0)?).map_err(|_| Error::CustomStr("Not UTF-8"))
    }

    /// The size to write is the len of `buff`.
    ///
    /// `offset` is the offset from the start of the file where to start writing.
    ///
    /// Return the total of bytes written or an error
    #[allow(unused_variables)]
    fn write(&self, offset: usize, buff: &Buffer) -> Result<usize, Error> {
        Err(Error::Fs(FsError::ReadOnly))
    }
}

/// Safety: any object implementing this trait should only be used inside a `FsNode`
pub unsafe trait Directory: Debug + Send + Sync {
    /// Find a file (or directory) with its `name`.
    fn find(&self, name: &str) -> Result<Option<FsNodeRef>, Error>;

    /// List all files in the directory.
    fn list(&self) -> Result<Vec<String>, Error>;
}

/// Safety: any object implementing this trait should only be used inside a `FsNode`
pub unsafe trait Block: Debug + Send + Sync {
    fn block_size(&self) -> usize;

    fn get_block(&self, block: BlockIndex) -> Result<BlockRef<'_>, Error>;
    fn get_block_mut(&self, block: BlockIndex) -> Result<BlockMut<'_>, Error>;

    /// Write a whole block without reading it first.
    ///
    /// `buff.len` should equal to the block size.
    fn write_block(&self, block: BlockIndex, buff: &Buffer) -> Result<(), Error>;
}
