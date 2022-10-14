use core::mem::MaybeUninit;

use crate::memory::mmu::invalidate_tlb_all;
use cortex_a::registers::TTBR0_EL1;
use tock_registers::interfaces::Writeable;
use uefi::table::boot::MemoryDescriptor;

mod constants;
mod heap;
mod mmu;
mod pmm;
pub mod vmm;

pub use pmm::physical;
pub use vmm::{vmm, MemoryUsage};

use self::pmm::PmmPageAllocator;

pub type PhysicalAddress = usize;
pub type VirtualAddress = usize;

#[global_allocator]
static mut ALLOCATOR: heap::Allocator = heap::Allocator::new();
static mut PMM_PAGE_ALLOCATOR: MaybeUninit<PmmPageAllocator> = MaybeUninit::uninit();

pub fn init(memory_map: &'static [MemoryDescriptor]) {
    unsafe {
        pmm::init(memory_map);
        PMM_PAGE_ALLOCATOR.write(PmmPageAllocator::new(
            &pmm::PHYSICAL_MANAGER.as_ref().unwrap_unchecked(),
        ));

        vmm::init(PMM_PAGE_ALLOCATOR.assume_init_ref());

        ALLOCATOR.init(vmm());

        TTBR0_EL1.set(0); // clear
        invalidate_tlb_all();
    }
}

// custom memory types defined in memory map by loader
#[derive(Debug)]
#[allow(unused)]
#[repr(u32)]
pub enum CustomMemoryTypes {
    Kernel = 0x80000000,
    MemoryMap = 0x80000001,
}

pub trait PageAllocator {
    unsafe fn alloc(&self, count: usize) -> *mut u8;
    unsafe fn dealloc(&self, ptr: usize, count: usize);
}
