use crate::memory::PhysicalAddress;

mod initrd;
mod mount;
mod open;
mod vfs;

pub use mount::mount;
pub use open::open;
pub use vfs::*;

#[inline]
pub unsafe fn init(initrd_ptr: PhysicalAddress, initrd_len: usize) {
    initrd::load(initrd_ptr, initrd_len);
}
