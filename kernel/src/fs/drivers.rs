use core::fmt::Debug;

use hashbrown::HashMap;
use log::trace;
use spin::{lazy::Lazy, lock_api::RwLock};

use crate::error::Error;

use super::node::FsNodeRef;

pub trait Driver: Send + Sync + Debug {
    fn fs_type<'a>(&'a self) -> &'a str;

    /// Get the root node of the filesystem on `device`. This is the node that will be mounted.
    fn get_root_node(&self, device: &FsNodeRef) -> Result<FsNodeRef, Error>;
}

static DRIVERS: Lazy<RwLock<HashMap<&'static str, &'static dyn Driver>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

pub fn register_driver(driver: &'static dyn Driver) {
    trace!(target: "fs", "Registering driver for {}", driver.fs_type());
    let mut drivers = DRIVERS.write();
    drivers.insert(driver.fs_type(), driver);
}

pub fn get_driver_for_type(fs_type: &str) -> Option<&dyn Driver> {
    DRIVERS.read().get(fs_type).copied()
}
