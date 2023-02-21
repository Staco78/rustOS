use core::{fmt::Debug, mem::MaybeUninit, slice};

use alloc::{vec::Vec, string::String};

use crate::{
    error::{Error, FsError::*},
    utils::smart_ptr::SmartPtr,
};

pub type FsNodeRef = SmartPtr<dyn FsNode>;

pub trait FsNode: Send + Sync + Debug {
    /// Return the size of the file in bytes.
    fn size(&self) -> Result<usize, Error> {
        Err(Error::Fs(NotImplemented("Getting size")))
    }

    /// The size to read is the len of `buff`.
    ///
    /// `offset` is the offset from the start of the file where to start reading.
    ///
    /// Return the buffer initialized or an error. The returned buffer len is the total of bytes read.
    fn read<'a>(
        &self,
        offset: usize,
        buff: &'a mut [MaybeUninit<u8>],
    ) -> Result<&'a mut [u8], Error> {
        let _ = (offset, buff);
        Err(Error::Fs(NotImplemented("Reading")))
    }

    fn read_vec(&self, offset: usize, size: usize) -> Result<Vec<u8>, Error> {
        let mut vec: Vec<u8> = Vec::with_capacity(size);
        let buff = unsafe {
            let ptr = vec.as_mut_ptr().cast::<MaybeUninit<u8>>();

            // Safety: the ptr is valid because vec allocated it for us
            slice::from_raw_parts_mut(ptr, size)
        };

        let buff_init = self.read(offset, buff)?;
        debug_assert!(buff_init.len() <= size);

        // Safety: elements are initalized by the file read
        unsafe { vec.set_len(buff_init.len()) };

        Ok(vec)
    }

    fn read_to_end_vec(&self, offset: usize) -> Result<Vec<u8>, Error> {
        if offset >= self.size()? {
            return Ok(Vec::new());
        }

        let to_read = self.size()? - offset;
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
    fn write(&self, offset: usize, buff: &[u8]) -> Result<usize, Error> {
        let _ = (offset, buff);
        Err(Error::Fs(NotImplemented("Writing")))
    }

    /// Find a file (or directory) with its `name`
    fn find(&self, name: &str) -> Result<Option<FsNodeRef>, Error> {
        let _ = name;
        Err(Error::Fs(NotImplemented("Finding")))
    }

    /// List all files in the directory.
    fn list(&self) -> Result<Vec<String>, Error> {
        Err(Error::Fs(NotImplemented("Listing")))
    }
}
