#![no_std]

#[cfg(not(feature = "kernel"))]
mod defs;
#[cfg(not(feature = "kernel"))]
mod heap;

#[cfg(not(feature = "kernel"))]
pub use defs::*;

#[cfg(not(feature = "kernel"))]
pub use modules_macros::module;

#[cfg(not(feature = "kernel"))]
pub extern crate alloc;

#[cfg(feature = "kernel")]
pub use modules_macros::export;

pub trait Module: Sync {
    fn init(&self) -> Result<(), ()>;
}

#[cfg(not(feature = "kernel"))]
#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    unsafe { panic(info) };
}

#[cfg(not(feature = "kernel"))]
#[no_mangle]
fn __module_pre_init() {
    let _ = log::set_logger(unsafe { KERNEL_LOGGER });
    log::set_max_level(log::STATIC_MAX_LEVEL);
}
