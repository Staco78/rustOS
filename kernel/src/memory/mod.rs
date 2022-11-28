use crate::{memory::mmu::invalidate_tlb_all, utils::sync_once_cell::SyncOnceCell};
use cortex_a::registers::TTBR0_EL1;
use log::info;
use tock_registers::interfaces::Writeable;
use uefi::table::boot::MemoryDescriptor;

mod addr_space;
mod constants;
mod heap;
mod mmu;
mod pmm;
pub mod vmm;

pub use addr_space::*;
pub use constants::*;
pub use vmm::{vmm, MemoryUsage};

use self::pmm::PmmPageAllocator;

pub type PhysicalAddress = usize;
pub type VirtualAddress = usize;

#[global_allocator]
static mut ALLOCATOR: heap::Allocator = heap::Allocator::new();
pub static PMM_PAGE_ALLOCATOR: SyncOnceCell<PmmPageAllocator> = SyncOnceCell::new();

pub fn init(memory_map: &'static [MemoryDescriptor]) {
    unsafe {
        pmm::init(memory_map);
        PMM_PAGE_ALLOCATOR
            .set(PmmPageAllocator::new(
                pmm::PHYSICAL_MANAGER.as_ref().unwrap_unchecked(),
            ))
            .unwrap();

        vmm::init(PMM_PAGE_ALLOCATOR.get().unwrap());

        ALLOCATOR.init(vmm());

        TTBR0_EL1.set(0); // clear
        invalidate_tlb_all();
    }
    info!(target: "memory", "Memory initialized");
}

// custom memory types defined in memory map by loader
#[derive(Debug)]
#[allow(unused)]
#[repr(u32)]
pub enum CustomMemoryTypes {
    Kernel = 0x80000000,
    MemoryMap = 0x80000001,
    KernelStack = 0x80000002,
    Dtb = 0x80000003,
    Initrd = 0x80000004,
}

pub trait PageAllocator: Sync {
    unsafe fn alloc(&self, count: usize) -> *mut u8;
    unsafe fn dealloc(&self, ptr: usize, count: usize);
}

#[inline]
pub const fn round_to_page_size(x: usize) -> usize {
    ((x + PAGE_SIZE - 1) >> PAGE_SHIFT) << PAGE_SHIFT
}
