mod cpu_interface;
mod distributor;
mod msi;
mod regs;

use spin::Once;

use crate::{
    device_tree,
    interrupts::{self, CoreSelection, InterruptMode, InterruptsChip},
    memory::PhysicalAddress,
};

use self::{cpu_interface::CpuInterface, distributor::Distributor, msi::MsiExtension};

pub struct GenericInterruptController {
    cpu_interface: CpuInterface,
    distributor: Distributor,
}

impl GenericInterruptController {
    fn new() -> (Self, Option<MsiExtension>) {
        let intc = device_tree::get_node_weak("/intc").expect("No intc node in DTB");
        assert!(intc.get_property("interrupt-controller").is_some());
        let mut reg = intc.get_property("reg").expect("No reg property").buff();

        assert_eq!(intc.address_cells(), 2);
        assert_eq!(intc.size_cells(), 2);

        let distributor_addr = {
            let a = reg.consume_be_u32().unwrap() as usize;
            let b = reg.consume_be_u32().unwrap() as usize;
            a << 32 | b
        };
        reg.advance_by(8).unwrap();
        let cpu_interface_addr = {
            let a = reg.consume_be_u32().unwrap() as usize;
            let b = reg.consume_be_u32().unwrap() as usize;
            a << 32 | b
        };

        let distributor_addr = PhysicalAddress::new(distributor_addr);
        let cpu_interface_addr = PhysicalAddress::new(cpu_interface_addr);

        let msi = if let Some(v2m) = intc.children().find(|n| n.name().starts_with("v2m@")) {
            let addr = v2m
                .get_property("reg")
                .unwrap()
                .buff()
                .consume_be_u64()
                .unwrap();
            let addr = PhysicalAddress::new(addr as usize);
            Some(MsiExtension::new(addr))
        } else {
            None
        };

        let s = Self {
            cpu_interface: CpuInterface::new(cpu_interface_addr),
            distributor: Distributor::new(distributor_addr),
        };

        s.distributor.init();
        s.cpu_interface.init();

        (s, msi)
    }
}

impl InterruptsChip for GenericInterruptController {
    #[inline]
    fn init_ap(&self) {
        self.cpu_interface.init();
    }

    #[inline]
    fn enable_interrupt(&self, interrupt: u32) {
        self.distributor.enable_interrupt(interrupt);
    }

    #[inline]
    fn disable_interrupt(&self, interrupt: u32) {
        self.distributor.disable_interrupt(interrupt)
    }

    #[inline]
    fn get_current_intid(&self) -> u32 {
        self.cpu_interface.get_current_intid()
    }

    #[inline]
    fn end_of_interrupt(&self, interrupt: u32) {
        self.cpu_interface.eoi(interrupt);
    }

    #[inline]
    fn send_sgi(&self, destination: CoreSelection, interrupt_id: u8) {
        self.distributor.send_sgi(destination, interrupt_id);
    }

    #[inline]
    fn set_mode(&self, interrupt: u32, mode: InterruptMode) {
        self.distributor.set_mode(interrupt, mode);
    }
}

pub fn init() {
    static CHIP: Once<(GenericInterruptController, Option<MsiExtension>)> = Once::new();
    let (chip, msi) = CHIP.call_once(|| GenericInterruptController::new());
    interrupts::register_chip(chip);
    if let Some(msi) = msi {
        interrupts::register_msi_chip(msi);
    }
}
