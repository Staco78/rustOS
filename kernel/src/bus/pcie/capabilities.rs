use log::debug;
use static_assertions::assert_eq_size;

use super::{Location, PciBar, PciDevice, PCI_CAPABILITIES};

pub struct CapabilitiesIter {
    loc: Location,
    offset: u8,
}

impl CapabilitiesIter {
    pub fn new(loc: Location) -> Self {
        Self {
            loc,
            offset: loc.read_u8(PCI_CAPABILITIES),
        }
    }
}

impl Iterator for CapabilitiesIter {
    type Item = Capability;
    fn next(&mut self) -> Option<Self::Item> {
        if self.offset == 0 {
            return None;
        }

        let reg = self.loc.read_u16(self.offset as usize);
        let new_off = (reg >> 8) as u8;
        let off = self.offset as usize;
        self.offset = new_off & !0b11;
        unsafe { Some(Capability::from_id(self.loc, off)) }
    }
}

#[derive(Debug, Clone)]
pub enum Capability {
    PowerManagement(&'static PowerManagementCapability),
    Msi(&'static MsiCapability),
    Pcie(&'static PcieCapability),
    Msix(&'static MsixCapability),
    Other(u8),
}

impl Capability {
    unsafe fn from_id(loc: Location, off: usize) -> Self {
        let id = loc.read_u8(off);
        match id {
            0x01 => Self::PowerManagement(&*loc.addr(off).as_ptr()),
            0x05 => Self::Msi(&*loc.addr(off).as_ptr()),
            0x10 => Self::Pcie(&*loc.addr(off).as_ptr()),
            0x11 => Self::Msix(&*loc.addr(off).as_ptr()),
            _ => Self::Other(id),
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone)]
struct CapabilityHeader {
    id: u8,
    next_ptr: u8,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct PcieCapability {
    header: CapabilityHeader,
    pcie_capabilities: u16,
    device_capabilities: u32,
    device_control: u16,
    device_status: u16,
    link_capabilities: u32,
    link_control: u16,
    link_status: u16,
    slot_capabilities: u32,
    slot_control: u16,
    slot_status: u16,
    root_control: u16,
    root_capabilities: u16,
    root_status: u32,
    device_capabilities_2: u32,
    device_control_2: u16,
    device_status_2: u16,
    link_capabilities_2: u32,
    link_control_2: u16,
    link_status_2: u16,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct PowerManagementCapability {
    header: CapabilityHeader,
    capabilities: u16,
    status_and_control: u16,
    pm_control_status: u8,
    data: u8,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct MsiCapability {
    header: CapabilityHeader,
    message_control: u16,
    message_addr: u32,
    message_addr_up: u32,
    message_data: u16,
    __: u16,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct MsixCapability {
    header: CapabilityHeader,
    message_control: u16,
    table_off_and_bar: u32,
    pba_off_and_bar: u32,
}

impl MsixCapability {
    #[inline]
    pub fn enable(&mut self) {
        self.message_control |= 0x8000;
        self.message_control &= !0x4000;
    }

    #[inline]
    /// Return the BAR index and the offset into it where the table is.
    pub fn table(&self) -> (usize, usize) {
        let bar = self.table_off_and_bar & 0b111;
        let off = self.table_off_and_bar & !0b111;
        (bar as usize, off as usize)
    }

    #[inline(always)]
    pub fn table_len(&self) -> usize {
        (self.message_control & 0x7FF) as usize + 1
    }
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct MsixTableEntry {
    pub addr: u64,
    pub data: u32,
    pub vector_control: u32,
}

assert_eq_size!(MsixTableEntry, u128);

impl MsixTableEntry {
    #[inline]
    pub fn unmask(&mut self) {
        self.vector_control &= !1;
    }
    #[inline]
    pub fn mask(&mut self) {
        self.vector_control |= 1;
    }
}
