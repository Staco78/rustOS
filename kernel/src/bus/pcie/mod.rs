#![allow(unused)]

use core::{
    fmt::{Debug, Display},
    ptr,
};

use alloc::vec::Vec;
use bitflags::bitflags;
use log::{debug, warn};
use spin::Once;

use crate::{
    device_tree, devices,
    memory::{PhysicalAddress, VirtualAddress},
    modules,
};

mod capabilities;
pub use capabilities::*;

// The below constants define the PCI configuration space.
// More info here: <http://wiki.osdev.org/PCI#PCI_Device_Structure>
const PCI_VENDOR_ID: usize = 0x0;
const PCI_DEVICE_ID: usize = 0x2;
const PCI_COMMAND: usize = 0x4;
const PCI_STATUS: usize = 0x6;
const PCI_REVISION_ID: usize = 0x8;
const PCI_PROG_IF: usize = 0x9;
const PCI_SUBCLASS: usize = 0xA;
const PCI_CLASS: usize = 0xB;
const PCI_CACHE_LINE_SIZE: usize = 0xC;
const PCI_LATENCY_TIMER: usize = 0xD;
const PCI_HEADER_TYPE: usize = 0xE;
const PCI_BIST: usize = 0xF;
const PCI_BARS: [usize; 6] = [0x10, 0x14, 0x18, 0x1C, 0x20, 0x24];
const PCI_CARDBUS_CIS: usize = 0x28;
const PCI_SUBSYSTEM_VENDOR_ID: usize = 0x2C;
const PCI_SUBSYSTEM_ID: usize = 0x2E;
const PCI_EXPANSION_ROM_BASE: usize = 0x30;
const PCI_CAPABILITIES: usize = 0x34;
// 0x35 through 0x3B are reserved
const PCI_INTERRUPT_LINE: usize = 0x3C;
const PCI_INTERRUPT_PIN: usize = 0x3D;
const PCI_MIN_GRANT: usize = 0x3E;
const PCI_MAX_LATENCY: usize = 0x3F;

#[inline]
fn infos() -> &'static Infos {
    static INFOS: Once<Infos> = Once::new();
    INFOS
        .try_call_once(extract_dtb_infos)
        .expect("Failed parsing DTB Pci infos")
}

fn buses() -> &'static Vec<PciBus> {
    static BUSES: Once<Vec<PciBus>> = Once::new();
    BUSES.call_once(scan_bus)
}

#[derive(Debug)]
struct Infos {
    ecam_addr: VirtualAddress,
}

fn extract_dtb_infos() -> Result<Infos, ()> {
    let node = device_tree::get_node_weak("/pcie").ok_or(())?;
    let reg = node.get_property("reg").ok_or(())?;
    let addr = reg.buff().consume_be_u64().unwrap();
    let addr = PhysicalAddress::new(addr as usize);

    let infos = Infos {
        ecam_addr: addr.to_virt(),
    };

    Ok(infos)
}

#[derive(Clone, Copy)]
pub struct Location {
    bus: u8,
    dev: u8,
    func: u8,
}

impl Location {
    fn new(bus: u8, dev: u8, func: u8) -> Self {
        debug_assert!(dev < 32);
        debug_assert!(func < 8);
        Self { bus, dev, func }
    }
    fn addr(&self, off: usize) -> VirtualAddress {
        debug_assert!(off < 0x1000);
        let addr = infos().ecam_addr;
        let bus = self.bus as usize;
        let dev = self.dev as usize;
        let func = self.func as usize;
        let off = bus << 20 | dev << 15 | func << 12 | off;
        addr + off
    }

    #[inline]
    unsafe fn read<T>(&self, off: usize) -> T {
        let ptr = self.addr(off).as_ptr();
        ptr::read_volatile(ptr)
    }

    #[inline]
    fn read_u8(&self, off: usize) -> u8 {
        unsafe { self.read(off) }
    }

    #[inline]
    fn read_u16(&self, off: usize) -> u16 {
        debug_assert!(off % 2 == 0);
        unsafe { self.read(off) }
    }

    #[inline]
    fn read_u32(&self, off: usize) -> u32 {
        debug_assert!(off % 4 == 0);
        unsafe { self.read(off) }
    }

    #[inline]
    unsafe fn write<T>(&self, off: usize, val: T) {
        let ptr = self.addr(off).as_ptr();
        ptr::write_volatile(ptr, val)
    }

    #[inline]
    fn write_u8(&self, off: usize, val: u8) {
        unsafe { self.write(off, val) }
    }

    #[inline]
    fn write_u16(&self, off: usize, val: u16) {
        debug_assert!(off % 2 == 0);
        unsafe { self.write(off, val) }
    }

    #[inline]
    fn write_u32(&self, off: usize, val: u32) {
        debug_assert!(off % 4 == 0);
        unsafe { self.write(off, val) }
    }
}

impl Debug for Location {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:02x}:{:02x}.{}", self.bus, self.dev, self.func)
    }
}

impl Display for Location {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        <Self as Debug>::fmt(self, f)
    }
}

bitflags! {
    pub struct Command: u16 {
        const IO_SPACE                  = 0x0001;
        const MEMORY_SPACE              = 0x0002;
        const BUS_MASTER                = 0x0004;
        const SPECIAL_CYCLES            = 0x0008;
        const MWI_ENABLE                = 0x0010;
        const VGA_PALETTE_SNOOP         = 0x0020;
        const PARITY_ERROR_RESPONSE     = 0x0040;
        const STEPPING_CONTROL          = 0x0080;
        const SERR_ENABLE               = 0x0100;
        const FAST_BACK_TO_BACK_ENABLE  = 0x0200;
        const INTERRUPT_DISABLE         = 0x0400;
        const RESERVED_11               = 0x0800;
        const RESERVED_12               = 0x1000;
        const RESERVED_13               = 0x2000;
        const RESERVED_14               = 0x4000;
        const RESERVED_15               = 0x8000;
    }
}

bitflags! {
    pub struct Status: u16 {
        const RESERVED_0                = 0x0001;
        const RESERVED_1                = 0x0002;
        const RESERVED_2                = 0x0004;
        const INTERRUPT_STATUS          = 0x0008;
        const CAPABILITIES_LIST         = 0x0010;
        const MHZ66_CAPABLE             = 0x0020;
        const RESERVED_6                = 0x0040;
        const FAST_BACK_TO_BACK_CAPABLE = 0x0080;
        const MASTER_DATA_PARITY_ERROR  = 0x0100;
        const DEVSEL_MEDIUM_TIMING      = 0x0200;
        const DEVSEL_SLOW_TIMING        = 0x0400;
        const SIGNALED_TARGET_ABORT     = 0x0800;
        const RECEIVED_TARGET_ABORT     = 0x1000;
        const RECEIVED_MASTER_ABORT     = 0x2000;
        const SIGNALED_SYSTEM_ERROR     = 0x4000;
        const DETECTED_PARITY_ERROR     = 0x8000;
    }
}

#[derive(Debug)]
pub struct PciBus {
    bus_number: u8,
    devices: Vec<PciDevice>,
}

#[derive(Debug, Clone)]
pub struct PciDevice {
    location: Location,

    vendor_id: u16,
    device_id: u16,

    class: u8,
    subclass: u8,
    prog_if: u8,
    revision: u8,

    command: Command,
    status: Status,

    header_full: HeaderType,
}

impl PciDevice {
    pub fn bars(&self) -> impl Iterator<Item = PciBar> + '_ {
        match &self.header_full {
            HeaderType::Common(header) => header.bars.iter().filter_map(|b| *b),
            _ => unimplemented!(),
        }
    }

    pub fn capabilities(&self) -> Option<CapabilitiesIter> {
        if self.status.contains(Status::CAPABILITIES_LIST) {
            let iter = CapabilitiesIter::new(self.location);
            Some(iter)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub enum HeaderType {
    Common(CommonHeader),
    PciToPci(()),
    PciToCardBus(()),
    Unknown,
}

#[derive(Debug, Clone)]
pub struct CommonHeader {
    bars: [Option<PciBar>; 6],
    cardbus_cis_ptr: u32,
    subsystem_id: u16,
    subsystem_vendor_id: u16,
    expansion_rom_addr: u32,
    capabilities_ptr: u8,
    interrupt_line: u8,
    interrupt_pin: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct PciBar {
    pub bar_type: PciBarType,
    pub addr: PhysicalAddress,
    pub size: usize,
}

#[derive(Debug, Clone, Copy)]
pub enum PciBarType {
    /// (locatable, prefetchable)
    Memory(PciBarMemoryLocatable, bool),
    IO,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum PciBarMemoryLocatable {
    Any32,
    Below1MB,
    Any64,
}

fn get_bars(loc: Location) -> [Option<PciBar>; 6] {
    const NONE_INIT: Option<PciBar> = None;
    let mut bars = [NONE_INIT; 6];
    let mut i = 0;
    while i < 6 {
        let val = loc.read_u32(PCI_BARS[i]);
        loc.write_u32(PCI_BARS[i], !0);
        let size = loc.read_u32(PCI_BARS[i]);
        loc.write_u32(PCI_BARS[i], val);

        if val == 0 && size == 0 {
            i += 1;
            continue;
        }

        let masked = if size & 1 == 1 {
            size & 0xFFFFFFFC
        } else {
            size & 0xFFFFFFF0
        };

        let mut size = (!masked).wrapping_add(1) as usize;

        if val & 1 == 0 {
            // Memory
            let locatable = (val >> 1) & 3;
            let locatable = match locatable {
                0 => PciBarMemoryLocatable::Any32,
                1 => PciBarMemoryLocatable::Below1MB,
                2 => PciBarMemoryLocatable::Any64,
                _ => continue,
            };
            let prefetchable = val & 0b1000 != 0;
            let mut addr = (val & !0b1111) as usize;
            let bar_ref = &mut bars[i];
            if locatable == PciBarMemoryLocatable::Any64 {
                let val_up = loc.read_u32(PCI_BARS[i + 1]);
                addr |= ((val_up as usize) << 32);
                i += 1;
            } else if locatable == PciBarMemoryLocatable::Below1MB {
                size &= 0xFFF0;
            }
            let addr = PhysicalAddress::new(addr);
            *bar_ref = Some(PciBar {
                bar_type: PciBarType::Memory(locatable, prefetchable),
                addr,
                size,
            });
        } else {
            // IO
            let addr = val & !0b11;
            let addr = PhysicalAddress::new(addr as usize);
            bars[i] = Some(PciBar {
                bar_type: PciBarType::IO,
                addr,
                size: size & 0xFFFFFFFC,
            });
        }
        i += 1;
    }

    bars
}

fn scan_bus() -> Vec<PciBus> {
    let mut buses = Vec::new();
    for bus in 0..=255 {
        let mut devices = Vec::new();
        for dev in 0..32 {
            let loc = Location::new(bus, dev, 0);
            let vendor = loc.read_u16(PCI_VENDOR_ID);
            if vendor == 0xFFFF {
                continue;
            }

            let header_type = loc.read_u8(PCI_HEADER_TYPE);
            let max_func = if header_type & 0x80 == 0 { 1 } else { 8 };

            for func in 0..max_func {
                let loc = Location::new(bus, dev, func);

                let vendor = loc.read_u16(PCI_VENDOR_ID);
                if vendor == 0xFFFF {
                    continue;
                }

                let header_type = loc.read_u8(PCI_HEADER_TYPE);
                let header = match header_type {
                    0 => HeaderType::Common(CommonHeader {
                        bars: get_bars(loc),
                        cardbus_cis_ptr: loc.read_u32(PCI_CARDBUS_CIS),
                        subsystem_id: loc.read_u16(PCI_SUBSYSTEM_ID),
                        subsystem_vendor_id: loc.read_u16(PCI_SUBSYSTEM_VENDOR_ID),
                        expansion_rom_addr: loc.read_u32(PCI_EXPANSION_ROM_BASE),
                        capabilities_ptr: loc.read_u8(PCI_CAPABILITIES),
                        interrupt_line: loc.read_u8(PCI_INTERRUPT_LINE),
                        interrupt_pin: loc.read_u8(PCI_INTERRUPT_PIN),
                    }),
                    // TODO: headers structs for both
                    1 => HeaderType::PciToPci(()),
                    2 => HeaderType::PciToCardBus(()),
                    _ => HeaderType::Unknown,
                };

                let device = PciDevice {
                    location: loc,
                    vendor_id: vendor,
                    device_id: loc.read_u16(PCI_DEVICE_ID),
                    class: loc.read_u8(PCI_CLASS),
                    subclass: loc.read_u8(PCI_SUBCLASS),
                    prog_if: loc.read_u8(PCI_PROG_IF),
                    revision: loc.read_u8(PCI_REVISION_ID),
                    command: Command::from_bits_truncate(loc.read_u16(PCI_COMMAND)),
                    status: Status::from_bits_truncate(loc.read_u16(PCI_STATUS)),
                    header_full: header,
                };
                devices.push(device);
            }
        }
        if !devices.is_empty() {
            buses.push(PciBus {
                bus_number: bus,
                devices,
            });
        }
    }
    buses
}

fn load_driver(dev: &PciDevice) {
    let driver = match (dev.class, dev.subclass, dev.prog_if) {
        (1, 8, _) => {
            // NVMe
            Some(("nvme", "/initrd/nvme.kmod"))
        }
        _ => None,
    };
    if let Some((device_type, driver)) = driver {
        let r = modules::load(driver);
        if let Err(e) = r {
            warn!(target: "pci", "Unable to load module {}: {}", driver, e);
        }
        devices::register_device(device_type, dev);
    }
}

pub fn init() {
    let buses = buses();
    for bus in buses {
        for dev in &bus.devices {
            load_driver(dev);
        }
    }
}
