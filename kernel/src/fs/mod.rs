use crate::memory::PhysicalAddress;

mod drivers;
mod initrd;
pub mod path;
mod vfs;
mod utils;

pub use vfs::*;
pub use drivers::*;
pub use utils::*;

#[inline]
/// Safety: `initrd_ptr` and `initrd_len` should be valid.
pub unsafe fn init(initrd_ptr: PhysicalAddress, initrd_len: usize) {
    initrd::init(initrd_ptr, initrd_len);
}
