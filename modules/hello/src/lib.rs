#![no_std]

use log::debug;
use module::*;

#[module]
static MOD: Mod = Mod {};

struct Mod {}

impl Module for Mod {
    fn init(&self) -> Result<(), ()> {
        debug!("Hello");
        Ok(())
    }
}
