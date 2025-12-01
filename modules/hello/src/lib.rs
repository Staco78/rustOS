#![no_std]
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

#[unsafe(no_mangle)]
pub static MODULE_NAME: &str = env!("CARGO_PKG_NAME");

#[unsafe(no_mangle)]
pub fn init() -> Result<(), Error> {
    debug!("Hello");
    error!("hey");
    logger::puts("hey logger\n");
    debug!("hey log");

    let x = 'T'.to_ascii_lowercase();
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
