use core::fmt::Debug;

use crate::{memory::mmu::invalidate_tlb_all, utils::sync_once_cell::SyncOnceCell};
use cortex_a::registers::TTBR0_EL1;
use log::info;
use module::export;
use tock_registers::interfaces::Writeable;
use uefi::table::boot::MemoryDescriptor;

mod addr_space;
mod address;
mod constants;
mod heap;
mod mmu;
mod pmm;
pub mod vmm;

pub use addr_space::*;
pub use address::{PhysicalAddress, VirtualAddress};
pub use constants::*;
pub use vmm::{vmm, MemoryUsage};

use self::{
    address::{Address, MemoryKind},
    pmm::PmmPageAllocator,
};

#[global_allocator]
static ALLOCATOR: heap::Allocator = heap::Allocator::new();
#[export]
static KERNEL_ALLOCATOR: &'static (dyn core::alloc::GlobalAlloc + Sync) = &ALLOCATOR;
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

pub trait PageAllocator<K: MemoryKind>: Sync + Debug {
    fn alloc(&self, count: usize) -> Option<Address<K>>;
    unsafe fn dealloc(&self, ptr: Address<K>, count: usize);
}
