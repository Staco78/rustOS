#![no_std]
#![feature(default_alloc_error_handler)]

use alloc::vec::Vec;
use log::debug;
use module::*;

#[module]
static MOD: Mod = Mod {};

struct Mod {}

impl Module for Mod {
    fn init(&self) -> Result<(), ()> {
        debug!("Hello");

        let mut x = Vec::new();
        x.resize(30, 4);
        debug!("{:?}", x);

        Ok(())
    }
}
