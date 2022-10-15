use tock_registers::interfaces::ReadWriteable;

use crate::memory::{
    vmm::{phys_to_virt, vmm, MapFlags, MapOptions, MapSize},
    VirtualAddress,
};

use super::regs::{CpuInterfaceRegs, GICC_CTLR, GICC_PMR};

pub struct CpuInterface {
    base: VirtualAddress,
}

impl CpuInterface {
    pub fn new(base: VirtualAddress) -> Self {
        assert!(base != 0);
        Self { base }
    }

    #[inline]
    fn regs(&self) -> &CpuInterfaceRegs {
        unsafe {
            &*(phys_to_virt(self.base) as *const CpuInterfaceRegs)
        }
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

        self.regs().ctlr.modify(GICC_CTLR::EnableGrp0::SET); // enable
        self.regs().pmr.modify(GICC_PMR::Priority.val(0xFF)); // accept all priorities
    }
}
