#![no_std]
#![feature(default_alloc_error_handler)]
#![feature(maybe_uninit_uninit_array)]
#![feature(maybe_uninit_slice)]
#![feature(maybe_uninit_array_assume_init)]
#![feature(int_roundings)]
#![feature(new_uninit)]

extern crate alloc;

mod driver;
mod filesystem;
mod structs;
mod consts;
mod nodes;

use driver::DRIVER;
use kernel::{error::Error, fs};

#[no_mangle]
pub fn init() -> Result<(), Error> {
    fs::register_driver(&DRIVER);
    Ok(())
}
