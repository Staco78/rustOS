mod cpu_interface;
mod distributor;
mod regs;

use crate::{acpi::madt::{Madt, MadtEntryType, MadtTable, GICC, GICD}, interrupts::interrupts::InterruptsManager};

use self::{cpu_interface::CpuInterface, distributor::Distributor};


pub struct GenericInterruptController {
    cpu_interface: CpuInterface,
    distributor: Distributor,
}

impl GenericInterruptController {
    pub fn new(madt: &Madt) -> Self {
        let mut cpu_interface_addr = None;
        let mut distributor_addr = None;

        for table in madt.iter() {
            match table.struct_type {
                MadtEntryType::GICC => {
                    let gicc = GICC::from_header(table).expect("Invalid GICC struct found");
                    if cpu_interface_addr.is_none() {
                        cpu_interface_addr = Some(gicc.base_addr as usize)
                    } else {
                        assert!(
                            cpu_interface_addr.unwrap() == gicc.base_addr as usize,
                            "Multiple GIC CPU interface base address found"
                        );
                    }
                }
                MadtEntryType::GICD => {
                    let gicd = GICD::from_header(table).expect("Invalid GICD struct found");
                    assert!(
                        distributor_addr.is_none(),
                        "More than one GICD struct found"
                    );
                    assert!(gicd.base_address != 0);
                    assert!(gicd.version == 2, "Only support GICv2");
                    distributor_addr = Some(gicd.base_address as usize)
                }
                _ => {}
            }
        }

        Self {
            cpu_interface: CpuInterface::new(
                cpu_interface_addr.expect("GIC CPU interface base address not found"),
            ),
            distributor: Distributor::new(
                distributor_addr.expect("GIC Distributor base address not found"),
            ),
        }
    }
}

impl InterruptsManager for GenericInterruptController {
    fn init(&mut self) {
        self.distributor.init();
        self.cpu_interface.init();
    }
}
