// SPCR is Microsoft Serial Port Console Redirection Table
// read more: https://docs.microsoft.com/en-us/windows-hardware/drivers/serports/serial-port-console-redirection-table

use static_assertions::assert_eq_size;

use super::{sdt::SdtHeader, AcpiGenericAddress};

#[derive(Debug, Clone, Copy)]
#[allow(unused)]
pub enum BaudRate {
    Unknown = 0,

    _9600 = 3,
    _19200,
    _57600,
    _115200,
}

assert_eq_size!(BaudRate, u8);

#[derive(Debug, Clone, Copy)]
#[allow(unused)]
pub enum TerminalType {
    VT100 = 0,
    ExtendedVT100,
    VtUtf8,
    ANSI,
}

assert_eq_size!(TerminalType, u8);

#[derive(PartialEq, Eq)]
pub enum SerialType {
    Unknown,
    // revision 1
    Full16550,
    Full16450,

    Subset16550,
    Max311xE,
    Pl011UART, // arm
    Msm8x60,
    Nvidia16550,
    TiOmap,
    Apm88xxxx,
    Msm8974,
    Sam5250,
    IntelUsif,
    Imx6,
    Sbsa, // arm generic uart
    Dcc, // arm
    Bcm2835,
    Sdm845,
    IntelLpss,
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct Spcr {
    pub header: SdtHeader,
    pub interface_type: u8,
    __: [u8; 3],
    pub address: AcpiGenericAddress,
    pub interrupt_type: u8,
    pub irq: u8,
    pub global_system_interrupt: u32,
    pub baud_rate: BaudRate,
    pub parity: u8,
    pub stop_bits: u8,
    pub flow_control: u8,
    pub terminal_type: TerminalType,
    pub language: u8,
    pub pci_device_id: u16,
    pub pci_vendor_id: u16,
    pub pci_bus_number: u8,
    pub pci_device_number: u8,
    pub pci_function_number: u8,
    pub pci_flags: u32,
    pub pci_segment: u8,
    pub uart_clock_frequency: u32,
}


impl Spcr {
    pub fn get_serial_type(&self) -> SerialType {
        if self.header.revision == 1 {
            match self.interface_type {
                0 => SerialType::Full16550,
                1 => SerialType::Full16450,
                _ => SerialType::Unknown
            }
        }
        else {
            match self.interface_type {
                0 => SerialType::Full16550,
                1 => SerialType::Subset16550,
                2 => SerialType::Max311xE,
                3 => SerialType::Pl011UART,
                4 => SerialType::Msm8x60,
                5 => SerialType::Nvidia16550,
                6 => SerialType::TiOmap,
                8 => SerialType::Apm88xxxx,
                9 => SerialType::Msm8974,
                10 => SerialType::Sam5250,
                11 => SerialType::IntelUsif,
                12 => SerialType::Imx6,
                13 | 14 => SerialType::Sbsa,
                15 => SerialType::Dcc,
                16 => SerialType::Bcm2835,
                17 => SerialType::Sdm845,
                18 => SerialType::Full16550,
                19 => SerialType::Sdm845,
                20 => SerialType::IntelLpss,
                _ => SerialType::Unknown
            }
        }
    }
}