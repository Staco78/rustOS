use tock_registers::interfaces::{ReadWriteable, Readable, Writeable};

use crate::memory::{
    vmm::{vmm, MapFlags, MapOptions, MapSize},
    AddrSpaceSelector, PhysicalAddress, VirtualAddress,
};

use super::regs::{CpuInterfaceRegs, GICC_CTLR, GICC_IAR, GICC_PMR};

pub struct CpuInterface {
    base: VirtualAddress,
}

impl CpuInterface {
    pub fn new(base: PhysicalAddress) -> Self {
        assert!(!base.is_null());
        vmm()
            .map_page(
                base.to_virt(),
                base,
                MapOptions::new(MapSize::Size4KB, MapFlags::new(false, false, 0b11, 2, true)),
                AddrSpaceSelector::kernel(),
            )
            .unwrap();

        Self {
            base: base.to_virt(),
        }
    }

    #[inline]
    fn regs(&self) -> &CpuInterfaceRegs {
        unsafe { &*(self.base.as_ptr() as *const CpuInterfaceRegs) }
    }

    pub fn init(&self) {
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
