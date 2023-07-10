#![no_std]
#![feature(default_alloc_error_handler)]
#![feature(unicode_internals)]
extern crate alloc;

use alloc::vec::Vec;
use kernel::{
    error::Error,
    logger,
    memory::{
        vmm::{vmm, MapFlags},
        AddrSpaceSelector, MemoryUsage,
    },
};
use log::{debug, error};

#[no_mangle]
pub static MODULE_NAME: &str = env!("CARGO_PKG_NAME");

#[no_mangle]
pub fn init() -> Result<(), Error> {
    debug!("Hello");
    error!("hey");
    logger::puts("hey logger\n");
    debug!("hey log");

    let x = core::unicode::conversions::to_lower('T');
    debug!("{:?}", x);

    let mut x = Vec::new();
    x.resize(30, 4);
    debug!("{:?}", x);

    let addr = vmm()
        .alloc_pages(
            15,
            MemoryUsage::KernelHeap,
            MapFlags::default(),
            AddrSpaceSelector::kernel(),
        )
        .unwrap();
    debug!("alloc at {:?}", addr);

    Ok(())
}
