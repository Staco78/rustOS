use crate::memory::PhysicalAddress;

mod initrd;
mod vfs;
mod mount;
mod open;

pub use mount::mount;
pub use open::open;

#[inline]
pub unsafe fn init(initrd_ptr: PhysicalAddress, initrd_len: usize) {
    initrd::load(initrd_ptr, initrd_len);
}