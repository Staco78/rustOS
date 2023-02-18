#![no_std]
#![no_main]
#![feature(panic_info_message)]
#![feature(strict_provenance)]
#![feature(pointer_byte_offsets)]
#![feature(default_alloc_error_handler)]
#![feature(sync_unsafe_cell)]
#![feature(cstr_from_bytes_until_nul)]
#![feature(pointer_is_aligned)]
#![feature(let_chains)]
#![feature(maybe_uninit_write_slice)]
#![feature(int_roundings)]
#![feature(const_trait_impl)]
#![feature(const_cmp)]
#![feature(unsize)]
#![feature(coerce_unsized)]
#![feature(new_uninit)]
#![feature(error_in_core)]

pub mod acpi;
pub mod cpu;
pub mod device_tree;
pub mod devices;
pub mod error;
pub mod fs;
pub mod interrupts;
pub mod logger;
pub mod memory;
pub mod modules;
pub mod psci;
pub mod scheduler;
pub mod symbols;
pub mod timer;
pub mod utils;

extern crate alloc;

use core::{
    fmt::Write,
    mem::{self, MaybeUninit},
    slice,
};

use acpi::AcpiParser;
use cortex_a::registers::{CurrentEL, DAIF};
use devices::pl011_uart;
use interrupts::exceptions;
use memory::PhysicalAddress;
use scheduler::SCHEDULER;
use tock_registers::interfaces::Readable;
use uefi::table::{boot::MemoryDescriptor, cfg::ConfigTableEntry};

use crate::{
    acpi::{
        sdt::Signature,
        spcr::{self, Spcr},
    },
    scheduler::exit,
};

pub static mut ACPI_TABLES: MaybeUninit<AcpiParser> = MaybeUninit::uninit();
#[export_name = "start"]
#[no_mangle]
extern "C" fn main(
    config_tables_ptr: PhysicalAddress,
    config_table_len: usize,
    memory_map_ptr: PhysicalAddress,
    memory_map_len: usize,
    dtb_ptr: PhysicalAddress,
    dtb_len: usize,
    initrd_ptr: PhysicalAddress,
    initrd_len: usize,
) -> ! {
    logger::init();
    assert!(
        CurrentEL
            .read_as_enum::<CurrentEL::EL::Value>(CurrentEL::EL)
            .unwrap()
            == CurrentEL::EL::Value::EL1
    );
    assert!(
        DAIF.is_set(DAIF::D)
            && DAIF.is_set(DAIF::A)
            && DAIF.is_set(DAIF::I)
            && DAIF.is_set(DAIF::F)
    ); // assert expections are disabled
    exceptions::init();

    let config_tables = unsafe {
        slice::from_raw_parts(
            config_tables_ptr.to_virt().as_ptr::<ConfigTableEntry>(),
            config_table_len,
        )
    };
    unsafe { ACPI_TABLES.write(AcpiParser::parse_tables(config_tables).unwrap()) };
    let mut console_writer = unsafe {
        if let Some(table) = ACPI_TABLES
            .assume_init_read()
            .get_table::<Spcr>(Signature::SPCR)
        {
            if (*table).get_serial_type() == spcr::SerialType::Pl011UART {
                Some(pl011_uart::Pl011::new(
                    PhysicalAddress::new((*table).address.address as usize).to_virt(),
                ))
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
            memory_map_ptr.to_virt().as_ptr::<MemoryDescriptor>(),
            memory_map_len,
        )
    };
    memory::init(memory_map);
    unsafe { fs::init(initrd_ptr, initrd_len) };
    symbols::init();
    device_tree::load(dtb_ptr, dtb_len);
    psci::init();
    interrupts::init_chip(unsafe {
        ACPI_TABLES
            .assume_init_mut()
            .get_table(Signature::MADT)
            .unwrap()
    });

    {
        scheduler::register_cpus();
        SCHEDULER.init(); // will start other cores
        SCHEDULER.start(later_main);
    }
}

fn later_main() -> ! {
    modules::load("/initrd/hello.kmod").unwrap();
    exit(0);
}
