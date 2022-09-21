use core::mem::MaybeUninit;

use uefi::table::boot::MemoryDescriptor;

mod heap;
mod pmm;
mod vmm;

pub use pmm::physical;
pub use vmm::{vmm, MemoryUsage, VmmError};

use self::vmm::VmmPageAllocator;

type PhysicalAddress = usize;
type VirtualAddress = usize;

const PAGE_SIZE: usize = 0x1000; // 4KB

#[global_allocator]
static mut ALLOCATOR: heap::Allocator = heap::Allocator::new();
static mut PAGE_ALLOCATOR: MaybeUninit<VmmPageAllocator> = MaybeUninit::uninit();

pub fn init(memory_map: &'static [MemoryDescriptor]) {
    pmm::init(memory_map);
    unsafe {
        vmm::init(physical());
    }

    unsafe {
        PAGE_ALLOCATOR.write(VmmPageAllocator::new(
            vmm::VIRTUAL_MANAGER.as_ref().unwrap_unchecked(),
        ));
        ALLOCATOR.init(PAGE_ALLOCATOR.assume_init_ref());
    }
}

// custom memory types defined in memory map by loader
#[derive(Debug)]
#[allow(unused)]
pub enum CustomMemoryTypes {
    Kernel = 0x80000000,
    MemoryMap = 0x80000001,
}

pub trait PageAllocator {
    unsafe fn alloc(&self, count: usize) -> *mut u8;
    unsafe fn dealloc(&self, ptr: usize, count: usize);
}
