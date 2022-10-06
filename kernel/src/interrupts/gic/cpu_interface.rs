use crate::memory::{
    vmm::{phys_to_virt, vmm, MapFlags, MapOptions, MapSize},
    VirtualAddress,
};

use super::regs::GICC;

pub struct CpuInterface {
    base: VirtualAddress,
}

impl CpuInterface {
    pub fn new(base: VirtualAddress) -> Self {
        assert!(base != 0);
        Self { base }
    }

    fn read_reg(&self, reg: GICC) -> u32 {
        let infos = reg.infos();
        assert!(infos.readable);
        let ptr = (phys_to_virt(self.base) + infos.offset) as *const u32;
        unsafe { ptr.read_volatile() }
    }

    fn write_reg(&mut self, reg: GICC, value: u32) {
        let infos = reg.infos();
        assert!(infos.writable);
        let ptr = (phys_to_virt(self.base) + infos.offset) as *mut u32;
        unsafe { ptr.write_volatile(value) }
    }

    pub fn init(&mut self) {
        unsafe {
            vmm()
                .map_page(
                    phys_to_virt(self.base),
                    self.base,
                    MapOptions::new(MapSize::Size4KB, MapFlags::new(false, false, 0b11, 2, true)),
                    None,
                )
                .unwrap();
        }
        self.write_reg(GICC::CTLR, 1); // enable
        self.write_reg(GICC::PMR, 0xFF); // accept all priorities
    }
}
