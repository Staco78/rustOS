use super::regs::{DistributorRegs, GICD_CTLR};
use crate::{
    devices::gic_v2::regs::GICD_SGIR,
    interrupts::interrupts::CoreSelection,
    memory::{
        vmm::{vmm, MapFlags, MapOptions, MapSize},
        AddrSpaceSelector, PhysicalAddress, VirtualAddress,
    },
};
use tock_registers::interfaces::Writeable;

pub struct Distributor {
    base: VirtualAddress,
}

impl Distributor {
    pub fn new(base: PhysicalAddress) -> Self {
        assert!(!base.is_null());

        vmm()
            .map_page(
                base.to_virt(),
                base,
                MapOptions::new(MapSize::Size4KB, MapFlags::new(false, false, 0b00, 2, true)), // write acccess, no shareability, Device-nGnRnE memory
                AddrSpaceSelector::kernel(),
            )
            .unwrap();

        Self {
            base: base.to_virt(),
        }
    }

    #[inline]
    fn regs(&self) -> &DistributorRegs {
        unsafe { &*(self.base.as_ptr() as *const DistributorRegs) }
    }

    pub fn init(&self) {
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
