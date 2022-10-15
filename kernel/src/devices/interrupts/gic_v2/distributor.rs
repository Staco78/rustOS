use super::regs::{DistributorRegs, GICD_CTLR};
use crate::memory::{
    vmm::{phys_to_virt, vmm, MapFlags, MapOptions, MapSize},
    PhysicalAddress,
};
use tock_registers::interfaces::Writeable;

pub struct Distributor {
    base: PhysicalAddress,
}

impl Distributor {
    pub fn new(base: PhysicalAddress) -> Self {
        assert!(base != 0);
        Self { base }
    }

    #[inline]
    fn regs(&self) -> &DistributorRegs {
        unsafe { &*(phys_to_virt(self.base) as *const DistributorRegs) }
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

        self.regs().ctlr.write(GICD_CTLR::EnableGrp0::SET);
    }

    pub fn enable_interrupt(&mut self, interrupt: u32) {
        let n = interrupt / 32;
        self.regs().isenabler[n as usize].set(1 << (interrupt % 32));
    }

    pub fn disable_interrupt(&mut self, interrupt: u32) {
        let n = interrupt / 32;
        self.regs().icenabler[n as usize].set(1 << (interrupt % 32));
    }
}
