use alloc::{format, string::ToString, vec::Vec};
use spin::lock_api::RwLock;

use crate::{
    error::{
        Error,
        FsError::{Custom, CustomStr, NotFound},
    },
    fs::{drivers::get_driver_for_type, path::Path},
};

use super::node::FsNodeRef;

pub fn get_node<P>(path: P) -> Result<FsNodeRef, Error>
where
    P: AsRef<Path>,
{
    let path = path.as_ref();
    assert!(path.is_absolute(), "Relative path not supported yet");
    let mountpoint = get_mountpoint(path).ok_or(Error::Fs(NotFound))?;
    let path_in_mountpoint: &Path = path
        .strip_prefix(mountpoint.path.as_str())
        .expect("Open: path doesn't start with moutpoint path")
        .into();
    let mut current_node = mountpoint.root_node;

    if path_in_mountpoint.len() == 0 || path_in_mountpoint == '/' {
        return Ok(current_node);
    }

    debug_assert!(path_in_mountpoint.is_absolute());

    for path_part in path_in_mountpoint[1..].split('/') {
        // TODO: remove the unwrap().
        current_node = current_node
            .find(path_part)
            .unwrap()
            .ok_or(Error::Fs(NotFound))?;
    }
    Ok(current_node)
}

#[derive(Debug, Clone)]
pub struct MountPoint {
    pub path: &'static Path,
    pub root_node: FsNodeRef,
}

static MOUNTPOINTS: RwLock<Vec<MountPoint>> = RwLock::new(Vec::new());

pub fn mount_device<S>(path: S, device: FsNodeRef, fs_type: &str) -> Result<(), Error>
where
    S: Into<&'static Path>,
{
    let driver = match get_driver_for_type(fs_type) {
        Some(driver) => driver,
        None => {
            return Err(Error::Fs(Custom(format!(
                "Unknown filesystem type: {}",
                fs_type.to_string()
            ))))
        }
    };
    let root_node = driver.get_root_node(&device)?;
    mount_node(path, root_node)?;
    Ok(())
}

pub fn mount_node<S>(path: S, node: FsNodeRef) -> Result<(), Error>
where
    S: Into<&'static Path>,
{
    let mountpoint = MountPoint {
        path: path.into(),
        root_node: node,
    };
    let mut mountpoints = MOUNTPOINTS.write();
    let i = match mountpoints.binary_search_by(|m| m.path.cmp(mountpoint.path)) {
        Ok(_) => return Err(Error::Fs(CustomStr("Already mounted"))),
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
