#![no_std]

#[cfg(not(feature = "kernel"))]
mod defs;
#[cfg(not(feature = "kernel"))]
pub use defs::*;

#[cfg(not(feature = "kernel"))]
pub use modules_macros::module;

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
    log::set_logger(unsafe { get_logger() }).unwrap();
    log::set_max_level(log::STATIC_MAX_LEVEL);
}
