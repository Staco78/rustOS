use hashbrown::HashMap;
use kernel::{error::Error, utils::smart_ptr::SmartPtrResizableBuff};
use spin::lock_api::RwLock;

use crate::{
    consts::INODE_CACHE_GROUP_SIZE,
    filesystem::InodeIndex,
    structs::{Inode, InodeRef},
};

#[derive(Debug)]
pub struct InodeCache {
    data: SmartPtrResizableBuff<Inode, INODE_CACHE_GROUP_SIZE>,
    hash: RwLock<HashMap<InodeIndex, InodeRef>>,
}

impl InodeCache {
    pub fn new() -> Self {
        Self {
            data: SmartPtrResizableBuff::new(),
            hash: RwLock::new(HashMap::new()),
        }
    }

    pub fn get<F>(&self, index: InodeIndex, get_inode: F) -> Result<InodeRef, Error>
    where
        F: FnOnce() -> Result<Inode, Error>,
    {
        let hash = self.hash.read();
        if let Some(inode) = hash.get(&index) {
            return Ok(InodeRef::clone(inode));
        }
        drop(hash);

        let inode = get_inode()?;
        let mut hash = self.hash.write();
        // Chech if the inode has been put in cache while we was reading it.
        if let Some(found_inode) = hash.get(&index) {
            debug_assert_eq!(&**found_inode, &inode);
            return Ok(InodeRef::clone(found_inode));
        }

        let inode = self.data.insert(inode);
        hash.insert(index, InodeRef::clone(&inode));

        Ok(inode)
    }
}
