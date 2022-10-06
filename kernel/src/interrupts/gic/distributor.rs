use crate::memory::{
    vmm::{phys_to_virt, vmm, MapFlags, MapOptions, MapSize},
    PhysicalAddress,
};

use super::regs::GICD;

pub struct Distributor {
    base: PhysicalAddress,
}

impl Distributor {
    pub fn new(base: PhysicalAddress) -> Self {
        assert!(base != 0);
        Self { base }
    }

    fn read_reg(&self, reg: GICD) -> u32 {
        let infos = reg.infos();
        assert!(infos.readable);
        let ptr = (phys_to_virt(self.base) + infos.offset) as *const u32;
        unsafe { ptr.read_volatile() }
    }

    fn write_reg(&mut self, reg: GICD, value: u32) {
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
                    MapOptions::new(MapSize::Size4KB, MapFlags::new(false, false, 0b00, 2, true)), // write acccess, no shareability, Device-nGnRnE memory
                    None,
                )
                .unwrap();
        }

        self.write_reg(GICD::CTLR, 1); // enable
    }

    pub fn enable_interrupt(&mut self, interrupt: u32) {
        let n = interrupt / 32;
        self.write_reg(GICD::ISENABLER(n as u8), 1 << (interrupt % 32));
    }

    pub fn disable_interrupt(&mut self, interrupt: u32) {
        let n = interrupt / 32;
        self.write_reg(GICD::ICENABLER(n as u8), 1 << (interrupt % 32));
    }
}
