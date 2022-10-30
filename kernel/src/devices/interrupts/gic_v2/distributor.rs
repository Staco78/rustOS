use super::regs::{DistributorRegs, GICD_CTLR};
use crate::{
    devices::gic_v2::regs::GICD_SGIR,
    interrupts::interrupts::CoreSelection,
    memory::{
        vmm::{phys_to_virt, vmm, MapFlags, MapOptions, MapSize},
        AddrSpaceSelector, PhysicalAddress,
    },
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
        vmm()
            .map_page(
                phys_to_virt(self.base),
                self.base,
                MapOptions::new(MapSize::Size4KB, MapFlags::new(false, false, 0b00, 2, true)), // write acccess, no shareability, Device-nGnRnE memory
                AddrSpaceSelector::kernel(),
            )
            .unwrap();

        self.regs().ctlr.write(GICD_CTLR::EnableGrp0::SET);
    }

    pub fn enable_interrupt(&self, interrupt: u32) {
        let n = interrupt / 32;
        self.regs().isenabler[n as usize].set(1 << (interrupt % 32));
    }

    pub fn disable_interrupt(&self, interrupt: u32) {
        let n = interrupt / 32;
        self.regs().icenabler[n as usize].set(1 << (interrupt % 32));
    }

    pub fn send_sgi(&self, destination: CoreSelection, interrupt_id: u8) {
        assert!(interrupt_id < 16);
        let (list_filter, target_list) = match destination {
            CoreSelection::Mask(mask) => (0, mask),
            CoreSelection::Others => (1, 0),
            CoreSelection::Me => (2, 0),
        };
        self.regs().sgir.write(
            GICD_SGIR::TargetListFilter.val(list_filter)
                + GICD_SGIR::CpuTargetList.val(target_list as u32)
                + GICD_SGIR::ID.val(interrupt_id as u32),
        );
    }
}
