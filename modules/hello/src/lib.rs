#![no_std]
#![feature(default_alloc_error_handler)]
#![feature(unicode_internals)]
extern crate alloc;

use alloc::vec::Vec;
use kernel::{
    logger,
    memory::{vmm, AddrSpaceSelector, MemoryUsage}, error::Error,
};
use log::{debug, error};

#[no_mangle]
pub static MODULE_NAME: &str = env!("CARGO_PKG_NAME");

#[no_mangle]
pub fn init() -> Result<(), Error> {
    debug!("Hello");
    error!("hey");
    logger::log("hey logger\n").unwrap();
    debug!("hey log");

    let x = core::unicode::conversions::to_lower('T');
    debug!("{:?}", x);

    let mut x = Vec::new();
    x.resize(30, 4);
    debug!("{:?}", x);

    let addr = vmm()
        .alloc_pages(15, MemoryUsage::KernelHeap, AddrSpaceSelector::kernel())
        .unwrap();
    debug!("alloc at {:?}", addr);

    Ok(())
}
