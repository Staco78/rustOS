use super::{
    mount::get_mountpoint,
    vfs::FileOrDir::{self, Dir},
};

pub fn open(path: &str) -> Option<FileOrDir> {
    assert!(path.starts_with('/'));
    let mountpoint = get_mountpoint(path)?;
    let path_in_mountpoint = path.strip_prefix(&mountpoint.path).unwrap();
    assert!(path_in_mountpoint.starts_with('/'));

    let mut current_node = mountpoint.node;
    for path_part in path_in_mountpoint[1..].split('/') {
        if let Dir(dir) = current_node {
            current_node = dir.find(path_part)?;
        } else {
            return None;
        }
    }

    Some(current_node)
}
