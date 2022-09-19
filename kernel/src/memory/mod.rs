use uefi::table::boot::MemoryDescriptor;

mod pmm;
mod vmm;
mod heap;

pub use pmm::physical;
pub use vmm::{vmm, MemoryUsage, VmmError};

type PhysicalAddress = usize;
type VirtualAddress = usize;

const PAGE_SIZE: usize = 0x1000; // 4KB

pub fn init(memory_map: &'static [MemoryDescriptor]) {
    pmm::init(memory_map);
    unsafe {
        vmm::init(physical());
    }
}

// custom memory types defined in memory map by loader
#[derive(Debug)]
#[allow(unused)]
pub enum CustomMemoryTypes {
    Kernel = 0x80000000,
    MemoryMap = 0x80000001,
}
