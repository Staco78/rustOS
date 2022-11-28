use core::{fmt::Debug, mem::MaybeUninit, slice};

use alloc::vec::Vec;

pub trait FsNode: Send + Sync + Debug {
    fn name(&self) -> &str;
}

pub trait FileNode: FsNode {
    /// Return the size of the file in bytes.
    fn size(&self) -> usize;

    /// The size to read is the len of `buff`.
    ///
    /// `offset` is the offset from the start of the file where to start reading.
    ///
    /// Return the buffer initialized or an error. The returned buffer len is the total of bytes read.
    fn read<'a>(
        &self,
        offset: usize,
        buff: &'a mut [MaybeUninit<u8>],
    ) -> Result<&'a mut [u8], ReadError>;

    fn read_vec(&self, offset: usize, size: usize) -> Result<Vec<u8>, ReadError> {
        let mut vec: Vec<u8> = Vec::with_capacity(size);
        let buff = unsafe {
            let ptr = vec.as_mut_ptr().cast::<MaybeUninit<u8>>();

            // Safety: the ptr is valid because vec allocated it for us
            slice::from_raw_parts_mut(ptr, size)
        };

        let buff_init = self.read(offset, buff)?;
        assert!(buff_init.len() <= size);

        // Safety: elements are initalized by the file read
        unsafe { vec.set_len(buff_init.len()) };

        Ok(vec)
    }

    #[inline]
    fn read_to_end_vec(&self, offset: usize) -> Result<Vec<u8>, ReadError> {
        if offset >= self.size() {
            return Ok(Vec::new());
        }

        let to_read = self.size() - offset;
        self.read_vec(offset, to_read)
    }

    /// The size to write is the len of `buff`.
    ///
    /// `offset` is the offset from the start of the file where to start writing.
    ///
    /// Return the total of bytes written or an error
    fn write(&self, offset: usize, buff: &[u8]) -> Result<usize, WriteError>;
}

pub trait DirNode: FsNode {
    /// Find a file (or directory) with its `name`
    fn find(&self, name: &str) -> Option<FileOrDir>;

    fn list(&self) -> Vec<&str>;
}

#[derive(Debug, Clone, Copy)]
pub enum FileOrDir<'a> {
    File(&'a dyn FileNode),
    Dir(&'a dyn DirNode),
}

impl<'a> FileOrDir<'a> {
    #[inline]
    pub fn as_file(self) -> Option<&'a dyn FileNode> {
        match self {
            Self::File(r) => Some(r),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum ReadError {}

#[derive(Debug)]
pub enum WriteError {
    ReadOnly,
}
