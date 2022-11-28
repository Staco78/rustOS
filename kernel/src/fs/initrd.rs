use core::{ffi::CStr, fmt::Debug, mem::MaybeUninit, slice};

use alloc::vec::Vec;

use crate::memory::{vmm::phys_to_virt, PhysicalAddress};

use super::{
    mount::mount,
    vfs::{DirNode, FileNode, FileOrDir, FsNode, ReadError, WriteError},
};

#[repr(C, packed)]
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

struct Node<'a> {
    name: &'a str,
    data: &'a [u8],
}

impl<'a> FsNode for Node<'a> {
    #[inline]
    fn name(&self) -> &str {
        &self.name
    }
}

impl<'a> FileNode for Node<'a> {
    #[inline]
    fn size(&self) -> usize {
        self.data.len()
    }

    fn read<'b>(
        &self,
        offset: usize,
        buff: &'b mut [MaybeUninit<u8>],
    ) -> Result<&'b mut [u8], ReadError> {
        if offset >= self.size() {
            return Ok(&mut []);
        }
        let to_read = (self.size() - offset).min(buff.len());
        let end = offset + to_read;

        let buff = MaybeUninit::write_slice(&mut buff[..to_read], &self.data[offset..end]);

        Ok(buff)
    }

    fn write(&self, _: usize, _: &[u8]) -> Result<usize, WriteError> {
        Err(WriteError::ReadOnly)
    }
}

impl<'a> Debug for Node<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Node").field("name", &self.name).finish()
    }
}

impl<'a> PartialEq for Node<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.name.eq(other.name)
    }
}

impl<'a> Eq for Node<'a> {}

impl<'a> PartialOrd for Node<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.name.partial_cmp(other.name)
    }
}

impl<'a> Ord for Node<'a> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.name.cmp(other.name)
    }
}

struct RootNode<'a> {
    files: Vec<Node<'a>>,
}

impl<'a> RootNode<'a> {
    fn new(data: &'a [u8]) -> Self {
        let iter = TarIterator::new(data);
        let mut files: Vec<Node<'_>> = iter
            .map(|(h, data)| Node {
                name: h.name(),
                data,
            })
            .collect();
        files.sort();
        Self { files }
    }
}

impl<'a> FsNode for RootNode<'a> {
    fn name(&self) -> &str {
        ""
    }
}

impl<'a> DirNode for RootNode<'a> {
    fn find(&self, name: &str) -> Option<FileOrDir> {
        let index = self.files.binary_search_by(|f| f.name.cmp(name)).ok()?;
        let file = &self.files[index];
        Some(FileOrDir::File(file as _))
    }

    fn list(&self) -> Vec<&str> {
        self.files.iter().map(|f| f.name).collect()
    }
}

impl<'a> Debug for RootNode<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "RootNode with {} files", self.files.len())
    }
}

static mut ROOT: MaybeUninit<RootNode<'static>> = MaybeUninit::uninit();

/// **Safety**: ptr and len should be valid.
pub unsafe fn load(ptr: PhysicalAddress, len: usize) {
    let ptr = phys_to_virt(ptr) as *const u8;
    let data = slice::from_raw_parts(ptr, len);
    let root = RootNode::new(data);
    ROOT.write(root);
    mount("/initrd", FileOrDir::Dir(ROOT.assume_init_ref()));
}
