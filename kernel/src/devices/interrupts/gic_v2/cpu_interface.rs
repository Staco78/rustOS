use tock_registers::interfaces::{ReadWriteable, Readable, Writeable};

use crate::memory::{
    vmm::{phys_to_virt, vmm, MapFlags, MapOptions, MapSize},
    AddrSpaceSelector, VirtualAddress,
};

use super::regs::{CpuInterfaceRegs, GICC_CTLR, GICC_IAR, GICC_PMR};

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
        unsafe { &*(phys_to_virt(self.base) as *const CpuInterfaceRegs) }
    }

    pub fn init(&mut self) {
        vmm()
            .map_page(
                phys_to_virt(self.base),
                self.base,
                MapOptions::new(MapSize::Size4KB, MapFlags::new(false, false, 0b11, 2, true)),
                AddrSpaceSelector::kernel(),
            )
            .unwrap();

        self.regs().ctlr.modify(GICC_CTLR::EnableGrp0::SET); // enable
        self.regs().pmr.modify(GICC_PMR::Priority.val(0xFF)); // accept all priorities
    }

    #[inline]
    pub fn get_current_intid(&self) -> u32 {
        self.regs().iar.read(GICC_IAR::InterruptId)
    }

    #[inline]
    pub fn eoi(&self, interrupt: u32) {
        self.regs().eoir.set(interrupt);
    }
}
