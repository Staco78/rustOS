use core::mem::{MaybeUninit, size_of};

use alloc::boxed::Box;

use crate::{
    error::{Error, FsError},
    utils::buffer::Buffer,
};

use super::node::File;

/// Safety: `T` should be safe to transmute from [\[u8\]].
pub unsafe fn read_struct<T>(node: &dyn File, offset: usize) -> Result<T, Error> {
    let mut data = MaybeUninit::<T>::uninit();
    let buff = Buffer::from_slice_mut(data.as_bytes_mut());
    let bytes_read = node.read(offset, buff)?;
    if bytes_read < size_of::<T>() {
        Err(Error::IoError)
    } else {
        Ok(unsafe { data.assume_init() })
    }
}

/// Safety: `T` should be safe to transmute from [\[u8\]].
pub unsafe fn read_slice_boxed<T>(
    node: &dyn File,
    offset: usize,
    count: usize,
) -> Result<Box<[T]>, Error> {
    let mut data = Box::new_uninit_slice(count);
    let bytes = data.as_bytes_mut();
    let buff = Buffer::from_slice_mut(bytes);
    let r = node.read(offset, buff)?;
    if r == buff.len() {
        let data = unsafe { data.assume_init() };
        Ok(data)
    } else {
        Err(Error::Fs(FsError::EndOfFile))
    }
}
