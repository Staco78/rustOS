use alloc::{
    string::{String, ToString},
    sync::{Arc, Weak},
    vec::Vec,
};
use core::{ffi::CStr, fmt::Debug, slice};
use vfs::node::{Directory, File, FsNodeInfos};

use crate::{
    create_fs_node,
    error::Error,
    memory::PhysicalAddress,
    sync::no_irq_locks::NoIrqMutex,
    utils::{
        buffer::Buffer,
        byte_size::ByteSize,
        smart_ptr::{SmartBuff, SmartPtrBuff, SmartPtrSizedBuff},
    },
};

use super::{
    drivers::{self, register_driver},
    node::{FsNode, FsNodeRef},
    vfs,
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
    fn fs_type(&self) -> &str {
        "tar"
    }

    fn get_root_node(&self, _device: &FsNodeRef) -> Result<FsNodeRef, Error> {
        unimplemented!()
    }
}

#[derive(Debug)]
struct FileSystem {
    files: SmartPtrBuff<FsNode<Node>>,
    root_node_buff: SmartPtrSizedBuff<FsNode<RootNode>, 1>,
}

impl FileSystem {
    fn new(self_weak: Weak<Self>, data: &'static [u8]) -> Self {
        let iter = TarIterator::new(data).map(|(h, d)| Node::new(h.name(), d));
        let files = SmartPtrBuff::from_iter(
            iter.map(|f| create_fs_node!(f, FsNodeInfos { size: f.data.len() }, file: dyn File)),
        );
        let root_node_buff = SmartPtrSizedBuff::new(false);
        root_node_buff
            .insert(create_fs_node!(
                RootNode { fs: self_weak },
                FsNodeInfos { size: files.len() },
                directory: dyn Directory
            ))
            .expect("Not enought space in buff");

        Self {
            files,
            root_node_buff,
        }
    }
}

#[derive(Debug)]
struct RootNode {
    fs: Weak<FileSystem>,
}

unsafe impl Directory for RootNode {
    fn find(&self, name: &str) -> Result<Option<FsNodeRef>, Error> {
        let fs = self.fs.upgrade().unwrap();
        let file = fs.files.iter().find(|f| f.name == name);
        Ok(file.map(FsNodeRef::new))
    }

    fn list(&self) -> Result<Vec<String>, Error> {
        let fs = self.fs.upgrade().unwrap();
        let files = fs.files.iter().map(|f| f.name.to_string()).collect();
        Ok(files)
    }
}

struct Node {
    name: &'static str,
    data: &'static [u8],
}

impl Debug for Node {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Node")
            .field("name", &self.name)
            .field("data", &format_args!("{}", ByteSize(self.data.len())))
            .finish()
    }
}

impl Node {
    fn new(name: &'static str, data: &'static [u8]) -> Self {
        Self { name, data }
    }
}

unsafe impl File for Node {
    fn read(&self, offset: usize, buff: &mut Buffer) -> Result<usize, Error> {
        if offset >= self.data.len() {
            return Ok(0);
        }
        let to_read = (self.data.len() - offset).min(buff.len());
        let end = offset + to_read;

        buff.write(0, &self.data[offset..end]);

        Ok(to_read)
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

    vfs::mount_node("/initrd", FsNodeRef::new(fs.root_node_buff.get(0).unwrap()))
        .expect("Unable to mount initrd");
}
