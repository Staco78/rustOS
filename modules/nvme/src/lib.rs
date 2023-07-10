#![no_std]
#![feature(default_alloc_error_handler)]
#![feature(int_roundings)]
#![feature(let_chains)]

extern crate alloc;

use alloc::{sync::Arc, vec::Vec};
use device::Device;
use kernel::{bus::pcie::PciDevice, devices, error::Error, interrupts};
use spin::lock_api::RwLock;

mod cmd;
mod device;
mod identify;
mod namespace;
mod queues;
mod regs;

#[no_mangle]
pub fn init() -> Result<(), Error> {
    devices::register_device_handler("nvme", device_handler)
        .map_err(|_| Error::CustomStr("Another nvme driver loaded"))?;
    Ok(())
}

static DEVICES: RwLock<Vec<Arc<Device>>> = RwLock::new(Vec::new());

fn device_handler(device: &PciDevice) {
    let device = Device::new(device.clone());
    let mut devices = DEVICES.write();
    devices.push(Arc::new(device));
    drop(devices);
    let devices = DEVICES.read();
    let device = devices.last().unwrap();

    device.init().unwrap();
}

fn interrupt_handler(id: u32, dev_index: usize) {
    let device = &DEVICES.read()[dev_index];
    device.interrupt_handler(id);
}

fn set_interrupt_handler(id: u32, device: &Device) {
    let devices = DEVICES.read();
    let (device_id, _) = devices
        .iter()
        .enumerate()
        .find(|&(_, d)| d.as_ref() as *const _ == device as *const _)
        .expect("Device not in Vec yet");

    interrupts::set_simple_irq_handler(id, interrupt_handler, device_id);
}
