#![no_main]
#![no_std]

mod cpu;
mod memory;

extern crate alloc;

use core::{arch::asm, ptr::NonNull, slice};

use alloc::boxed::Box;
use elfloader::{
    ElfBinary, ElfLoader, ElfLoaderErr, Flags, LoadableHeaders, RelocationEntry, VAddr,
};
use log::info;
use memory::PAGE_SIZE;
use uefi::{
    CStr16,
    boot::{AllocateType, MemoryType, image_handle},
    mem::memory_map::{MemoryMap, MemoryMapMeta},
    prelude::*,
    proto::media::file::{File, FileAttribute, FileInfo, FileMode},
};

use crate::memory::{CustomMemoryTypes, VirtualAddress};

const KERNEL_STACK_PAGES_COUNT: usize = 16;
const KERNEL_LINEAR_MAP_OFFSET: usize = 0xFFFF_0000_0000_0000;

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();

    uefi::system::with_stdout(|stdout| stdout.clear()).unwrap();
    memory::init();

    let dtb = load_file(
        cstr16!("dtb.dtb"),
        MemoryType::custom(CustomMemoryTypes::Dtb as u32),
    );
    let initrd = load_file(
        cstr16!("initrd"),
        MemoryType::custom(CustomMemoryTypes::Initrd as u32),
    );

    let kernel_entry = load_kernel();
    let kernel_stack_addr = boot::allocate_pages(
        AllocateType::AnyPages,
        MemoryType::custom(CustomMemoryTypes::KernelStack as u32),
        KERNEL_STACK_PAGES_COUNT,
    )
    .unwrap()
    .addr()
    .get()
        + (KERNEL_STACK_PAGES_COUNT * PAGE_SIZE)
        + KERNEL_LINEAR_MAP_OFFSET;

    let memory_map = unsafe {
        boot::exit_boot_services(Some(MemoryType::custom(
            CustomMemoryTypes::MemoryMap as u32,
        )))
    };

    unsafe {
        info!("Running kernel...");

        system::with_config_table(|config_table| {
            let config_tables_ptr = config_table.as_ptr().addr();
            let config_table_len = config_table.len();

            let memory_map_ptr = memory_map.buffer().as_ptr().addr();
            let memory_map_len = memory_map.buffer().len();
            let memory_map_meta_ptr: *const MemoryMapMeta = &memory_map.meta();
            let dtb_ptr = dtb.as_ptr().addr();
            let dtb_len = dtb.len();
            let initrd_ptr = initrd.as_ptr().addr();
            let initrd_len = initrd.len();

            asm!(
                "mov sp, {}",
                "br {}",
                in(reg) kernel_stack_addr,
                in(reg) kernel_entry,
                in("x0") config_tables_ptr,
                in("x1") config_table_len,
                in("x2") memory_map_ptr,
                in("x3") memory_map_len,
                in("x4") memory_map_meta_ptr,
                in("x5") dtb_ptr,
                in("x6") dtb_len,
                in("x7") initrd_ptr,
                in("x8") initrd_len
            ); // this should never return
            unreachable!();
        });
        unreachable!();
    }
}

fn load_file(file_name: &CStr16, mem_type: MemoryType) -> &'static mut [u8] {
    let mut protocol = boot::get_image_file_system(image_handle()).unwrap();
    let mut protocol = protocol.get_mut();
    let mut volume = protocol.as_mut().unwrap().open_volume().unwrap();
    let file = volume
        .open(file_name, FileMode::Read, FileAttribute::SYSTEM)
        .unwrap();
    let mut file = file.into_regular_file().unwrap();
    let file_info: Box<FileInfo> = file.get_boxed_info().unwrap();
    let size = file_info.file_size() as usize;

    let buff = boot::allocate_pool(mem_type, size).unwrap();
    let buff = unsafe { slice::from_raw_parts_mut(buff.as_ptr(), size) };
    file.read(buff).unwrap();

    buff
}

// return entry point
fn load_kernel() -> u64 {
    let buff = load_file(cstr16!("kernel"), MemoryType::LOADER_DATA);
    let mut loader = KernelLoader;
    let binary = ElfBinary::new(buff).unwrap();
    binary.load(&mut loader).unwrap();
    let entry_point = binary.entry_point();
    unsafe { boot::free_pool(NonNull::from_ref(&buff[0])).unwrap() };
    entry_point
}

struct KernelLoader;

impl ElfLoader for KernelLoader {
    fn allocate(&mut self, load_headers: LoadableHeaders) -> Result<(), ElfLoaderErr> {
        for header in load_headers {
            let size = header.mem_size();
            let page_count = if size & 0xFFF == 0 {
                size / 0x1000
            } else {
                (size / 0x1000) + 1
            };
            let mut phys_addr = boot::allocate_pages(
                AllocateType::AnyPages,
                MemoryType::custom(CustomMemoryTypes::Kernel as u32),
                page_count as usize,
            )
            .unwrap()
            .addr()
            .get() as u64;
            let mut virt_addr = header.virtual_addr();
            for _ in 0..page_count {
                memory::map_page(VirtualAddress(virt_addr), phys_addr).unwrap();
                virt_addr += 0x1000;
                phys_addr += 0x1000;
            }
        }
        Ok(())
    }

    fn relocate(&mut self, _entry: RelocationEntry) -> Result<(), ElfLoaderErr> {
        unimplemented!();
    }

    fn load(&mut self, _flags: Flags, base: VAddr, region: &[u8]) -> Result<(), ElfLoaderErr> {
        unsafe { core::ptr::copy(region.as_ptr(), base as *mut u8, region.len()) };
        Ok(())
    }
}
