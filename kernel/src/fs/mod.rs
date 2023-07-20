use crate::memory::PhysicalAddress;

pub(crate) mod devfs;
mod drivers;
mod initrd;
pub mod path;
mod utils;
mod vfs;
pub mod block;

pub use drivers::*;
pub use utils::*;
pub use vfs::*;

#[inline]
/// Safety: `initrd_ptr` and `initrd_len` should be valid.
pub unsafe fn init(initrd_ptr: PhysicalAddress, initrd_len: usize) {
    initrd::init(initrd_ptr, initrd_len);
}
