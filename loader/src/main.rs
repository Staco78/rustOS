#![no_main]
#![no_std]
#![feature(abi_efiapi)]
#![feature(panic_info_message)]
#![allow(incomplete_features)]
#![allow(improper_ctypes_definitions)]

mod cpu;
mod memory;

extern crate alloc;

use core::{intrinsics::transmute, mem::size_of, slice};

use alloc::boxed::Box;
use elfloader::{
    ElfBinary, ElfLoader, ElfLoaderErr, Flags, LoadableHeaders, RelocationEntry, VAddr,
};
use log::info;
use uefi::{
    prelude::*,
    proto::media::file::{File, FileAttribute, FileInfo, FileMode},
    table::{
        boot::{AllocateType, MemoryDescriptor, MemoryType},
        cfg::ConfigTableEntry,
        Runtime,
    },
};

use crate::memory::{VirtualAddress, CustomMemoryTypes};

#[entry]
fn main(handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    uefi_services::init(&mut system_table).unwrap();
    let stdout = system_table.stdout();
    stdout.clear().unwrap();
    memory::init();

    let kernel_entry = load_kernel(handle, &system_table);
    let (system_table, memory_map) = exit_boot_services(handle, system_table);
    unsafe {
        let kernel_entry =
            transmute::<u64, extern "C" fn(&[ConfigTableEntry], &[MemoryDescriptor])>(kernel_entry);
        info!("Running kernel...");
        kernel_entry(system_table.config_table(), memory_map);
    }

    info!("Halting CPU");
    cpu::halt();
}

// exit boot services and return memory map
fn exit_boot_services(
    handle: Handle,
    system_table: SystemTable<Boot>,
) -> (SystemTable<Runtime>, &'static [MemoryDescriptor]) {
    let bt = system_table.boot_services();

    let mem_map_size = bt.memory_map_size();
    let buff_size = mem_map_size.map_size + 2 * mem_map_size.entry_size;
    let buff = bt
        .allocate_pool(MemoryType::custom(CustomMemoryTypes::MemoryMap as u32), buff_size)
        .unwrap();
    let real_map_size = bt.memory_map_size();
    assert!(real_map_size.entry_size >= size_of::<MemoryDescriptor>());
    let (system_table, map) = unsafe {
        system_table
            .exit_boot_services(handle, slice::from_raw_parts_mut(buff, buff_size))
            .unwrap()
    };
    let result = unsafe { slice::from_raw_parts_mut(buff as *mut MemoryDescriptor, map.len()) };

    let mut i = 0;
    for desc in map {
        result[i] = *desc;
        i += 1;
    }

    (system_table, result)
}

// return entry point
fn load_kernel(handle: Handle, system_table: &SystemTable<Boot>) -> u64 {
    let protocol = system_table
        .boot_services()
        .get_image_file_system(handle)
        .unwrap();
    let mut volume = unsafe { protocol.interface.get().as_mut().unwrap() }
        .open_volume()
        .unwrap();
    let file = volume
        .open(cstr16!("kernel"), FileMode::Read, FileAttribute::SYSTEM)
        .unwrap();
    let mut file = file.into_regular_file().unwrap();
    let file_info: Box<FileInfo> = file.get_boxed_info().unwrap();
    let size = file_info.file_size() as usize;

    let buff = system_table
        .boot_services()
        .allocate_pool(MemoryType::LOADER_DATA, size)
        .unwrap();
    assert!(!buff.is_null());
    let buff = unsafe { slice::from_raw_parts_mut(buff, size) };
    file.read(buff).unwrap();
    let mut loader = KernelLoader::new(system_table.boot_services());
    let binary = ElfBinary::new(buff).unwrap();
    binary.load(&mut loader).unwrap();
    let entry_point = binary.entry_point();
    system_table
        .boot_services()
        .free_pool(buff.as_mut_ptr())
        .unwrap();
    entry_point
}

struct KernelLoader<'a> {
    boot_services: &'a BootServices,
}

impl<'a> KernelLoader<'a> {
    fn new(boot_services: &'a BootServices) -> Self {
        KernelLoader { boot_services }
    }
}

impl<'a> ElfLoader for KernelLoader<'a> {
    fn allocate(&mut self, load_headers: LoadableHeaders) -> Result<(), ElfLoaderErr> {
        for header in load_headers {
            let size = header.mem_size();
            let page_count = if size & 0xFFF == 0 {
                size / 0x1000
            } else {
                (size / 0x1000) + 1
            };
            let mut phys_addr = self
                .boot_services
                .allocate_pages(
                    AllocateType::AnyPages,
                    MemoryType::custom(CustomMemoryTypes::Kernel as u32),
                    page_count as usize,
                )
                .unwrap();
            let mut virt_addr = header.virtual_addr();
            for _ in 0..page_count {
                memory::map_page(self.boot_services, VirtualAddress(virt_addr), phys_addr).unwrap();
                virt_addr += 0x1000;
                phys_addr += 0x1000;
            }
        }
        Ok(())
    }

    fn relocate(&mut self, _entry: RelocationEntry) -> Result<(), ElfLoaderErr> {
        todo!();
    }

    fn load(&mut self, _flags: Flags, base: VAddr, region: &[u8]) -> Result<(), ElfLoaderErr> {
        unsafe {
            self.boot_services
                .memmove(base as *mut u8, region.as_ptr(), region.len())
        };
        Ok(())
    }
}