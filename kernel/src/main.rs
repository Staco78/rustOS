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

mod acpi;
mod cpu;
mod device_tree;
mod devices;
mod fs;
mod interrupts;
mod logger;
mod memory;
mod psci;
mod scheduler;
mod timer;
mod utils;

extern crate alloc;

use core::{
    fmt::Write,
    mem::{self, MaybeUninit},
    slice, ffi::CStr,
};

use acpi::AcpiParser;
use cortex_a::registers::CurrentEL;
use devices::pl011_uart;
use interrupts::exceptions;
use log::debug;
use memory::PhysicalAddress;
use scheduler::{current_process, thread::Thread, SCHEDULER};
use tock_registers::interfaces::Readable;
use uefi::table::{boot::MemoryDescriptor, cfg::ConfigTableEntry};

use crate::{
    acpi::{
        sdt::Signature,
        spcr::{self, Spcr},
    },
    memory::vmm::{self, phys_to_virt},
    scheduler::exit,
};

pub static mut ACPI_TABLES: MaybeUninit<AcpiParser> = MaybeUninit::uninit();

#[export_name = "start"]
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
    exceptions::init();

    let config_tables = unsafe {
        slice::from_raw_parts(
            vmm::phys_to_virt(config_tables_ptr) as *const ConfigTableEntry,
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
            memory_map_len,
        )
    };
    memory::init(memory_map);
    unsafe { fs::init(initrd_ptr, initrd_len) };
    device_tree::load(dtb_ptr, dtb_len);
    psci::init();
    interrupts::init_chip(unsafe {
        ACPI_TABLES
            .assume_init_mut()
            .get_table(Signature::MADT)
            .unwrap()
    });

    {
        let my_id = scheduler::register_cpus();
        SCHEDULER.init(); // will start other cores
        SCHEDULER.start(my_id, later_main);
    }
}

fn later_main() -> ! {
    let file = fs::open("/initrd/tt").expect("not found");
    assert!(file.is_file());
    let mut buff = [0; 60];
    file.as_file().unwrap().read(0, &mut buff).unwrap();
    let str = CStr::from_bytes_until_nul(&buff).unwrap();
    debug!("{:?}", str);


    Thread::new(current_process(), other_thread, false)
        .unwrap()
        .start();
    exit(0);
}

fn other_thread() -> ! {
    loop {}
}
