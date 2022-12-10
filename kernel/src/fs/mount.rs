use alloc::vec::Vec;

use crate::utils::no_irq_locks::NoIrqRwLock;

use super::vfs::FileOrDir;

#[derive(Debug, Clone)]
pub struct MountPoint {
    pub path: &'static str,
    pub node: FileOrDir<'static>,
}

static MOUNTPOINTS: NoIrqRwLock<Vec<MountPoint>> = NoIrqRwLock::new(Vec::new());

pub fn mount(path: &'static str, node: FileOrDir<'static>) {
    let mut mountpoints = MOUNTPOINTS.write();
    let i = mountpoints
        .binary_search_by(|m| m.path.cmp(path))
        .expect_err("Already mounted");
    mountpoints.insert(i, MountPoint { path, node });
}

/// find in what mountpoint the path is
pub fn get_mountpoint(path: &str) -> Option<MountPoint> {
    let mountpoints = MOUNTPOINTS.read();

    let mut best: Option<&MountPoint> = None;
    for mountpoint in mountpoints.iter() {
        if path.starts_with(&mountpoint.path) {
            if let Some(best) = &mut best {
                if mountpoint.path.len() > best.path.len() {
                    *best = mountpoint;
                }
            } else {
                best = Some(mountpoint);
            }
        }
    }

    best.map(|m| m.clone())
}
