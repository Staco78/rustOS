use alloc::{
    sync::{Arc, Weak},
    vec::Vec,
};
use core::{
    ffi::CStr,
    fmt::Debug,
    mem::{transmute, MaybeUninit},
    slice,
};

use crate::{
    memory::PhysicalAddress,
    utils::{
        no_irq_locks::NoIrqMutex,
        smart_ptr::{SmartBuff, SmartPtr, SmartPtrBuff, SmartPtrSizedBuff},
    },
};

use super::{
    drivers::{self, register_driver},
    node::{FileNode, FileNodeRef},
    vfs, ReadError,
};

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct TarHeader {
    filename: [u8; 100],
    mode: u64,
    uid: u64,
    gid: u64,
    size: [u8; 12],
    mtime: [u8; 12],
    checksum: u64,
    typeflag: u8,
}

impl TarHeader {
    #[inline]
    fn size(&self) -> usize {
        let mut buff = [0; 13];
        buff[..12].copy_from_slice(&self.size);
        let size_str = CStr::from_bytes_until_nul(&buff).unwrap().to_str().unwrap();
        usize::from_str_radix(size_str, 8).unwrap()
    }

    #[inline]
    fn name(&self) -> &str {
        CStr::from_bytes_until_nul(&self.filename)
            .unwrap()
            .to_str()
            .unwrap()
    }
}

struct TarIterator<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> TarIterator<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
}

impl<'a> Iterator for TarIterator<'a> {
    type Item = (&'a TarHeader, &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        let header: *const TarHeader = unsafe { self.data.as_ptr().add(self.pos).cast() };
        debug_assert!(header.is_aligned());
        let header = unsafe { header.as_ref().unwrap_unchecked() };

        if header.filename[0] == 0 {
            return None;
        }

        self.pos += 512;
        let size = header.size();
        let data = &self.data[self.pos..self.pos + size];
        let size = size.next_multiple_of(512);
        self.pos += size;
        Some((header, data))
    }
}

#[derive(Debug)]
struct Driver {
    filesystems: NoIrqMutex<Vec<(Arc<FileSystem>, Vec<u8>)>>,
}

impl Driver {
    const fn new() -> Self {
        Self {
            filesystems: NoIrqMutex::new(Vec::new()),
        }
    }
}

impl drivers::Driver for Driver {
    fn fs_type<'a>(&'a self) -> &'a str {
        "tar"
    }

    fn get_root_node(&self, device: &FileNodeRef) -> Result<FileNodeRef, ()> {
        // TODO: remove unwrap
        let data = device.read_to_end_vec(0).unwrap();
        // Safety: No filesystem deletion so `data` never dropped.
        let data_ = unsafe { transmute::<&[u8], &'static [u8]>(&data) };
        let fs = Arc::new_cyclic(|weak| FileSystem::new(weak.clone(), data_));
        self.filesystems.lock().push((Arc::clone(&fs), data));

        Ok(fs.root_node().clone())
    }
}

#[derive(Debug)]
struct FileSystem {
    files: SmartPtrBuff<Node>,
    root_node_buff: SmartPtrSizedBuff<RootNode, 1>,
}

impl FileSystem {
    fn new(self_weak: Weak<Self>, data: &'static [u8]) -> Self {
        let iter = TarIterator::new(data).map(|(h, d)| Node::new(h.name(), d));
        let files = SmartPtrBuff::from_iter(iter);
        let root_node_buff = SmartPtrSizedBuff::new(false);
        root_node_buff
            .create_new(RootNode { fs: self_weak })
            .expect("Not enought space in buff");

        Self {
            files,
            root_node_buff,
        }
    }

    fn root_node(&self) -> SmartPtr<RootNode> {
        self.root_node_buff.get(0).expect("RootNode not created")
    }
}

#[derive(Debug)]
struct RootNode {
    fs: Weak<FileSystem>,
}

impl FileNode for RootNode {
    fn size(&self) -> Result<usize, ()> {
        Err(())
    }

    fn find(&self, name: &str) -> Result<Option<FileNodeRef>, ()> {
        let fs = self.fs.upgrade().unwrap();
        let file = fs.files.iter().find(|f| f.name == name);
        Ok(file.map(|f| f as FileNodeRef))
    }
}

#[derive(Debug)]
struct Node {
    name: &'static str,
    data: &'static [u8],
}

impl Node {
    fn new(name: &'static str, data: &'static [u8]) -> Self {
        Self { name, data }
    }
}

impl FileNode for Node {
    fn size(&self) -> Result<usize, ()> {
        Ok(self.data.len())
    }

    fn read<'a>(
        &self,
        offset: usize,
        buff: &'a mut [core::mem::MaybeUninit<u8>],
    ) -> Result<&'a mut [u8], ReadError> {
        if offset >= self.data.len() {
            return Ok(&mut []);
        }
        let to_read = (self.data.len() - offset).min(buff.len());
        let end = offset + to_read;

        let buff = MaybeUninit::write_slice(&mut buff[..to_read], &self.data[offset..end]);

        Ok(buff)
    }
}

static DRIVER: Driver = Driver::new();

/// Safety: `initrd_ptr` and `initrd_len` should be valid.
pub unsafe fn init(initrd_ptr: PhysicalAddress, initrd_len: usize) {
    register_driver(&DRIVER);

    let data: &[u8] = slice::from_raw_parts(initrd_ptr.to_virt().as_ptr(), initrd_len);
    let fs = Arc::new_cyclic(|w| FileSystem::new(w.clone(), data));
    let vec: Vec<u8> =
        unsafe { Vec::from_raw_parts(initrd_ptr.to_virt().as_ptr(), initrd_len, initrd_len) };
    DRIVER.filesystems.lock().push((fs.clone(), vec));

    vfs::mount_node("/initrd", fs.root_node_buff.get(0).unwrap()).expect("Unable to mount initrd");
}
