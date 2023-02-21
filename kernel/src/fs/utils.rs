use core::mem::{size_of, MaybeUninit};

use alloc::vec::Vec;

use crate::error::Error;

use super::node::FsNode;

/// Safety: `T` should be safe to transmute from [[u8]].
pub unsafe fn read_struct<T>(node: &dyn FsNode, offset: usize) -> Result<T, Error> {
    let mut buff = MaybeUninit::<T>::uninit();
    node.read(offset, buff.as_bytes_mut())?;
    Ok(buff.assume_init())
}

/// Safety: `T` should be safe to transmute from [[u8]].
pub unsafe fn read_struct_vec<T>(
    node: &dyn FsNode,
    offset: usize,
    count: usize,
) -> Result<Vec<T>, Error> {
    let vec = node.read_vec(offset, size_of::<T>() * count)?;
    let (ptr, length, capacity) = vec.into_raw_parts();

    debug_assert!(length == capacity);
    debug_assert!(length % size_of::<T>() == 0);

    let ptr = ptr as *mut T;
    let length = length / size_of::<T>();
    let capacity = capacity / size_of::<T>();

    let vec = Vec::from_raw_parts(ptr, length, capacity);
    Ok(vec)
}
