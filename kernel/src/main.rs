#![no_std]
#![no_main]
#![feature(lang_items)]
#![feature(panic_info_message)]
#![feature(strict_provenance)]
#![feature(const_mut_refs)]
#![feature(pointer_byte_offsets)]
#![feature(default_alloc_error_handler)]
#![allow(improper_ctypes_definitions)]

mod acpi;
mod cpu;
mod interrupts;
mod logger;
mod memory;
mod utils;

extern crate alloc;

use core::{
    fmt::Write,
    mem::{self, MaybeUninit},
};

use acpi::AcpiParser;
use devices::pl011_uart;
use interrupts::exceptions;
use uefi::table::{boot::MemoryDescriptor, cfg::ConfigTableEntry};

use crate::acpi::{
    sdt::Signature,
    spcr::{self, Spcr},
};

pub static mut ACPI_TABLES: MaybeUninit<AcpiParser> = MaybeUninit::uninit();

#[export_name = "start"]
extern "C" fn main(config_tables: &[ConfigTableEntry], memory_map: &'static [MemoryDescriptor]) {
    logger::init();
    exceptions::init();

    let acpi_parser = AcpiParser::parse_tables(config_tables).unwrap();
    unsafe { ACPI_TABLES.write(acpi_parser) };
    let mut console_writer = unsafe {
        if let Some(table) = ACPI_TABLES
            .assume_init_read()
            .get_table::<Spcr>(Signature::SPCR)
        {
            if (*table).get_serial_type() == spcr::SerialType::Pl011UART {
                Some(pl011_uart::Pl011::new((*table).address.address))
            } else {
                None
            }
        } else {
            None
        }
    };
    if let Some(writer) = &mut console_writer {
        logger::set_output(unsafe { mem::transmute(writer as &mut dyn Write) });
    }

    memory::init(memory_map);
}
