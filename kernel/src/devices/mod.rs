use hashbrown::HashMap;
use log::{trace, warn};

mod interrupts;
pub use interrupts::*;
mod serial;
pub use serial::*;

use spin::{lock_api::RwLock, Lazy};

use crate::bus::pcie::PciDevice;

pub type DeviceHandler = fn(&PciDevice);

static HANDLERS: Lazy<RwLock<HashMap<&'static str, DeviceHandler>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Register an handler for devices `device_type`. If there is already an handler, return `Err(())`.
pub fn register_device_handler(
    device_type: &'static str,
    handler: DeviceHandler,
) -> Result<(), ()> {
    let mut handlers = HANDLERS.write();
    if handlers.contains_key(device_type) {
        Err(())
    } else {
        handlers.insert(device_type, handler);
        trace!(target: "devices", "Registering handler for {} devices", device_type);
        Ok(())
    }
}

pub fn register_device(device_type: &'static str, device: &PciDevice) {
    let handlers = HANDLERS.read();
    if let Some(handler) = handlers.get(device_type) {
        handler(device);
    } else {
        warn!(target: "devices", "No handler for device type {}", device_type);
    }
}
