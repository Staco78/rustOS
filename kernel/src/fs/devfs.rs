use alloc::{string::String, vec::Vec};
use hashbrown::HashMap;
use log::error;
use spin::{lazy::Lazy, lock_api::RwLock};

use crate::{error::Error, fs::node::FsNodeInfos, create_fs_node, utils::smart_ptr::SmartPtr};

use super::{
    mount_node,
    node::{Directory, FsNode, FsNodeRef},
};

static DEVFS: Lazy<SmartPtr<FsNode<DevFs>>> = Lazy::new(|| {
    let dev = DevFs::new();
    let node = create_fs_node!(
        dev,
        FsNodeInfos { size: dev.size() },
        directory: dyn Directory
    );
    let ptr = SmartPtr::new_boxed(node);
    let node = FsNodeRef::new(SmartPtr::clone(&ptr));
    let r = mount_node("/dev", node);
    if let Err(e) = r {
        error!("Failed to mount devfs: {}", e);
    }
    ptr
});

#[derive(Debug)]
struct DevFs {
    nodes: RwLock<HashMap<String, FsNodeRef>>,
}

impl DevFs {
    fn new() -> Self {
        Self {
            nodes: RwLock::new(HashMap::new()),
        }
    }

    fn size(&self) -> usize {
        let nodes = self.nodes.read();
        nodes.len()
    }
}

unsafe impl Directory for DevFs {
    fn find(&self, name: &str) -> Result<Option<FsNodeRef>, Error> {
        let nodes = self.nodes.read();
        Ok(nodes.get(name).cloned())
    }

    fn list(&self) -> Result<Vec<String>, Error> {
        let nodes = self.nodes.read();
        let keys = nodes.keys().cloned().collect();
        Ok(keys)
    }
}

/// Add `device` into the devfs.
///
/// Panic if a device with the same `name` already exist.
pub fn add_device<S: Into<String>>(name: S, device: FsNodeRef) {
    let mut nodes = DEVFS.nodes.write();
    let r = nodes.insert(name.into(), device);
    assert!(r.is_none(), "Device already exist");
}
