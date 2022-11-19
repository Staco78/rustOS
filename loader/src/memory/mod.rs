mod mmu;

pub const PAGE_SIZE: usize = 4096;

pub struct VirtualAddress(pub u64);
pub type PhysicalAddress = u64;

impl VirtualAddress {
    fn get_l0_index(&self) -> usize {
        ((self.0 >> 39) & 0x1FF) as usize
    }

    fn get_l1_index(&self) -> usize {
        ((self.0 >> 30) & 0x1FF) as usize
    }

    fn get_l2_index(&self) -> usize {
        ((self.0 >> 21) & 0x1FF) as usize
    }

    fn get_l3_index(&self) -> usize {
        ((self.0 >> 12) & 0x1FF) as usize
    }
}

pub fn init() {
    mmu::init();
}

pub use mmu::map_page;

pub enum CustomMemoryTypes {
    Kernel = 0x80000000,
    MemoryMap = 0x80000001,
    KernelStack = 0x80000002,
    Dtb = 0x80000003,
    Initrd = 0x80000004,
}
