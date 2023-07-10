use super::regs::{DistributorRegs, GICD_CTLR};
use crate::{
    devices::gic_v2::regs::{GICD_SGIR, GICD_TYPER},
    interrupts::{CoreSelection, InterruptMode},
    memory::{
        vmm::{vmm, MapFlags, MapOptions, MapSize},
        AddrSpaceSelector, PhysicalAddress, VirtualAddress,
    },
};
use tock_registers::interfaces::{Readable, Writeable};

pub struct Distributor {
    base: VirtualAddress,
    irq_count: usize,
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

        let base = base.to_virt();

        let regs = unsafe { &*(base.as_ptr() as *const DistributorRegs) };
        let it_lines_count = regs.typer.read(GICD_TYPER::ITLinesCount);
        let irq_count = (it_lines_count + 1) * 32;

        Self {
            base,
            irq_count: irq_count as usize,
        }
    }

    #[inline(always)]
    fn regs(&self) -> &DistributorRegs {
        unsafe { &*(self.base.as_ptr() as *const DistributorRegs) }
    }

    pub fn init(&self) {
        for irq in 32..self.irq_count {
            // Set target to CPU 0
            let n = irq / 4;
            let off = (irq % 4) * 8;
            let reg = &self.regs().itargetsr[n];
            let val = reg.get();
            let val = val | 1 << off;
            reg.set(val);
        }

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

    pub fn set_mode(&self, interrupt: u32, mode: InterruptMode) {
        let reg = &self.regs().icfgr[interrupt as usize / 16];
        let val = reg.get();
        let off = (interrupt % 16) * 2 + 1;
        let val = match mode {
            InterruptMode::EdgeTriggered => val | 1 << off,
            InterruptMode::LevelSensitive => val & !(1 << off),
        };
        reg.set(val);
    }
}
