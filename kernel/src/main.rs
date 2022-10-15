#![no_std]
#![no_main]
#![feature(panic_info_message)]
#![feature(strict_provenance)]
#![feature(pointer_byte_offsets)]
#![feature(default_alloc_error_handler)]
#![allow(incomplete_features)]

mod acpi;
mod cpu;
mod devices;
mod interrupts;
mod logger;
mod memory;
mod utils;

extern crate alloc;

use core::{
    fmt::Write,
    mem::{self, MaybeUninit},
    slice,
};

use acpi::AcpiParser;
use cpu::halt;
use devices::{pl011_uart, gic_v2::GenericInterruptController};
use interrupts::{exceptions, interrupts::InterruptsManager};
use memory::PhysicalAddress;
use uefi::table::{boot::MemoryDescriptor, cfg::ConfigTableEntry};

use crate::{
    acpi::{
        sdt::Signature,
        spcr::{self, Spcr},
    },
    memory::vmm::{self, phys_to_virt},
};

pub static mut ACPI_TABLES: MaybeUninit<AcpiParser> = MaybeUninit::uninit();

#[export_name = "start"]
extern "C" fn main(
    config_tables_ptr: PhysicalAddress,
    config_table_len: u32,
    memory_map_ptr: PhysicalAddress,
    memory_map_len: u32,
) {
    logger::init();
    exceptions::init();

    let config_tables = unsafe {
        slice::from_raw_parts(
            vmm::phys_to_virt(config_tables_ptr) as *const ConfigTableEntry,
            config_table_len as usize,
        )
    };
    unsafe { ACPI_TABLES.write(AcpiParser::parse_tables(config_tables).unwrap()) };
    let mut console_writer = unsafe {
        if let Some(table) = ACPI_TABLES
            .assume_init_read()
            .get_table::<Spcr>(Signature::SPCR)
        {
            if (*table).get_serial_type() == spcr::SerialType::Pl011UART {
                Some(pl011_uart::Pl011::new(phys_to_virt(
                    (*table).address.address as usize,
                )))
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

    let memory_map = unsafe {
        slice::from_raw_parts(
            vmm::phys_to_virt(memory_map_ptr) as *const MemoryDescriptor,
            memory_map_len as usize,
        )
    };
    memory::init(memory_map);

    let mut gic = GenericInterruptController::new(unsafe {
        ACPI_TABLES
            .assume_init_mut()
            .get_table(Signature::MADT)
            .unwrap()
    });
    gic.init();

    halt();
}
