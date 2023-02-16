use alloc::vec::Vec;
use spin::lock_api::RwLock;

use crate::fs::{drivers::get_driver_for_type, path::Path};

use super::{node::FileNodeRef, OpenError};

pub fn get_node<P>(path: P) -> Result<FileNodeRef, OpenError>
where
    P: AsRef<Path>,
{
    let path = path.as_ref();
    assert!(path.is_absolute(), "Relative path not supported yet");
    let mountpoint = get_mountpoint(path).ok_or(OpenError::NotFound)?;
    let path_in_mountpoint: &Path = path
        .strip_prefix(mountpoint.path.as_str())
        .expect("Open: path doesn't start with moutpoint path")
        .into();
    debug_assert!(path_in_mountpoint.is_absolute());

    let mut current_node = mountpoint.root_node;

    for path_part in path_in_mountpoint[1..].split('/') {
        // TODO: remove the unwrap().
        current_node = current_node
            .find(path_part)
            .unwrap()
            .ok_or(OpenError::NotFound)?;
    }
    Ok(current_node)
}

#[derive(Debug, Clone)]
pub struct MountPoint {
    pub path: &'static Path,
    pub root_node: FileNodeRef,
}

static MOUNTPOINTS: RwLock<Vec<MountPoint>> = RwLock::new(Vec::new());

pub fn mount_device(path: &'static Path, device: FileNodeRef, fs_type: &str) -> Result<(), ()> {
    let driver = match get_driver_for_type(fs_type) {
        Some(driver) => driver,
        None => return Err(()), // Unknown fs type
    };
    let root_node = driver.get_root_node(&device)?;
    mount_node(path, root_node)?;
    Ok(())
}

pub fn mount_node<S>(path: S, node: FileNodeRef) -> Result<(), ()>
where
    S: Into<&'static Path>,
{
    let mountpoint = MountPoint {
        path: path.into(),
        root_node: node,
    };
    let mut mountpoints = MOUNTPOINTS.write();
    let i = match mountpoints.binary_search_by(|m| m.path.cmp(mountpoint.path)) {
        Ok(_) => return Err(()), // Already mounted
        Err(i) => i,
    };
    mountpoints.insert(i, mountpoint);
    Ok(())
}

/// Find in which filesytem the path is.
pub fn get_mountpoint(path: &str) -> Option<MountPoint> {
    let mountpoints = MOUNTPOINTS.read();

    let mut best: Option<&MountPoint> = None;
    for mountpoint in mountpoints.iter() {
        if path.starts_with(mountpoint.path.as_str()) {
            if let Some(best) = &mut best {
                if mountpoint.path.len() > best.path.len() {
                    *best = mountpoint;
                }
            } else {
                best = Some(mountpoint);
            }
        }
    }

    best.cloned()
}
