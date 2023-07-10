use alloc::{sync::Arc, vec::Vec};
use kernel::{
    error::Error,
    fs::{self, node::FsNodeRef},
};
use spin::lock_api::Mutex;

use crate::filesystem::FileSystem;

pub static DRIVER: Driver = Driver {
    filesystems: Mutex::new(Vec::new()),
};

#[derive(Debug)]
pub struct Driver {
    filesystems: Mutex<Vec<Arc<FileSystem>>>,
}

impl fs::Driver for Driver {
    fn fs_type(&self) -> &str {
        "ext2"
    }

    fn get_root_node(&self, device: &FsNodeRef) -> Result<FsNodeRef, Error> {
        let fs = FileSystem::new(device.clone())?;
        let root_node = fs.get_root_node()?;
        self.filesystems.lock().push(fs);
        Ok(root_node)
    }
}
