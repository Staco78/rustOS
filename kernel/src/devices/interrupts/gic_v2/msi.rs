use core::sync::atomic::{AtomicU32, Ordering};

use tock_registers::interfaces::Readable;

use crate::{
    interrupts::{MsiChip, MsiVector},
    memory::{
        vmm::{vmm, MapFlags, MapOptions, MapSize},
        AddrSpaceSelector, PhysicalAddress, VirtualAddress,
    },
};

use super::regs::{MsiRegs, MSI_TYPER};

/// Code for the GICv2m: GICv2 with MSI support.
pub struct MsiExtension {
    regs: &'static MsiRegs,
    base_spi: u32,
    spis_count: u32,
    next_free_spi: AtomicU32,
}

unsafe impl Send for MsiExtension {}
unsafe impl Sync for MsiExtension {}

impl MsiExtension {
    pub fn new(addr: PhysicalAddress) -> Self {
        let addr: VirtualAddress = vmm()
            .map_page(
                addr.to_virt(),
                addr,
                MapOptions::new(MapSize::Size4KB, MapFlags::new(false, false, 0b11, 2, true)),
                AddrSpaceSelector::kernel(),
            )
            .unwrap();

        let regs: &'static MsiRegs = unsafe { &*addr.as_ptr() };

        let typer = regs.typer.extract();
        let base_spi = typer.read(MSI_TYPER::BASE_SPI);
        let spis_count = typer.read(MSI_TYPER::SPI_COUNT);

        Self {
            regs,
            base_spi,
            spis_count,
            next_free_spi: AtomicU32::new(base_spi),
        }
    }
}

impl MsiChip for MsiExtension {
    fn get_free_vector(&self) -> Option<MsiVector> {
        let vector = self.next_free_spi.fetch_add(1, Ordering::Relaxed);
        if vector > (self.base_spi + self.spis_count) {
            return None;
        }

        let addr = VirtualAddress::from_ptr(&self.regs.setspi as *const _ as *mut u32);
        let addr = addr.to_phys().expect("Should be mapped").addr() as u64;

        let vector = MsiVector {
            interrupt: vector,
            addr,
            data: vector,
        };

        Some(vector)
    }
}
