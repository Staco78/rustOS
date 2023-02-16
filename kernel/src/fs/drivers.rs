use core::fmt::Debug;

use hashbrown::HashMap;
use spin::{lazy::Lazy, lock_api::RwLock};

use super::node::FileNodeRef;

pub trait Driver: Send + Sync + Debug {
    fn fs_type<'a>(&'a self) -> &'a str;

    /// Get the root node of the filesystem on `device`. This is the node that will be mounted.
    fn get_root_node(&self, device: &FileNodeRef) -> Result<FileNodeRef, ()>;
}

static DRIVERS: Lazy<RwLock<HashMap<&'static str, &'static dyn Driver>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

pub fn register_driver(driver: &'static dyn Driver) {
    let mut drivers = DRIVERS.write();
    drivers.insert(driver.fs_type(), driver);
}

pub fn get_driver_for_type(fs_type: &str) -> Option<&dyn Driver> {
    DRIVERS.read().get(fs_type).copied()
}
