#![no_std]
#![no_main]
#![feature(sync_unsafe_cell)]
#![feature(maybe_uninit_write_slice)]
#![feature(int_roundings)]
#![feature(const_trait_impl)]
#![feature(unsize)]
#![feature(coerce_unsized)]
#![feature(maybe_uninit_as_bytes)]
#![feature(assert_matches)]
#![feature(integer_atomics)]
#![feature(maybe_uninit_slice)]
#![feature(ptr_metadata)]
#![feature(never_type)]
#![feature(unwrap_infallible)]
#![feature(pointer_is_aligned_to)]
#![feature(unsafe_cell_access)]

pub mod acpi;
pub mod bus;
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
pub mod sync;
pub mod timer;
pub mod utils;

extern crate alloc;

use core::{fmt::Write, mem, slice};

use aarch64_cpu::registers::{CurrentEL, DAIF};
use devices::pl011_uart;
use interrupts::exceptions;
use memory::PhysicalAddress;
use scheduler::SCHEDULER;
use tock_registers::interfaces::Readable;
use uefi::{
    mem::memory_map::{MemoryMapMeta, MemoryMapRef},
    table::cfg::ConfigTableEntry,
};

use crate::{
    acpi::{
        sdt::Signature,
        spcr::{self, Spcr},
    },
    bus::pcie,
    devices::gic_v2,
    scheduler::exit,
};

#[unsafe(export_name = "start")]
extern "C" fn main(
    config_tables_ptr: PhysicalAddress,
    config_table_len: usize,
    memory_map_ptr: PhysicalAddress,
    memory_map_len: usize,
    memory_map_meta: *const MemoryMapMeta,
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

    let memory_map_meta = unsafe { *memory_map_meta };

    let dtb = unsafe { slice::from_raw_parts(dtb_ptr.to_virt().as_ptr(), dtb_len) };
    device_tree::init(dtb).expect("Dtb init failed");

    let config_tables = unsafe {
        slice::from_raw_parts(
            config_tables_ptr.to_virt().as_ptr::<ConfigTableEntry>(),
            config_table_len,
        )
    };
    unsafe { acpi::init(config_tables).unwrap() };

    // TODO: move this to another file
    let mut console_writer = unsafe {
        if let Some(table) = acpi::get_table::<Spcr>(Signature::SPCR) {
            if (*table).get_serial_type() == spcr::SerialType::Pl011UART {
                Some(pl011_uart::Pl011::new(
                    PhysicalAddress::new(table.address.address as usize).to_virt(),
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

    let memory_map_buff =
        unsafe { slice::from_raw_parts(memory_map_ptr.to_virt().as_ptr(), memory_map_len) };
    let memory_map = MemoryMapRef::new(memory_map_buff, memory_map_meta).unwrap();

    memory::init(memory_map);
    unsafe { fs::init(initrd_ptr, initrd_len) };
    symbols::init();
    psci::init();
    gic_v2::init();

    {
        scheduler::register_cpus();
        SCHEDULER.init(); // will start other cores
        SCHEDULER.start(later_main);
    }
}

fn later_main() -> ! {
    pcie::init();

    modules::load("/initrd/ext2.kmod").unwrap();

    exit(0);
}
