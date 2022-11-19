use core::fmt::Debug;

use alloc::vec::Vec;

pub trait FsNode: Send + Sync + Debug {
    fn name(&self) -> &str;
}

pub trait FileNode: FsNode {
    /// Return the size of the file un bytes.
    fn size(&self) -> usize;

    /// The size to read is the len of `buff`.
    ///
    /// `offset` is the offset from the start of the file where to start reading.
    ///
    /// Return the total of bytes read or an error
    fn read(&self, offset: usize, buff: &mut [u8]) -> Result<usize, ReadError>;

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
    pub fn is_file(self) -> bool {
        matches!(self, Self::File(_))
    }

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
