use core::{
    fmt::Debug,
    iter::{self},
    mem::MaybeUninit,
    sync::atomic::{AtomicUsize, Ordering},
};

use alloc::{boxed::Box, format, string::String, vec::Vec};
use spin::lock_api::Mutex;

use crate::{
    error::Error,
    fs::{devfs, node::FsNode},
    utils::{buffer::Buffer, smart_ptr::SmartPtr},
};

pub trait Disk: Send + Sync + Debug {
    fn block_size(&self) -> usize;
    fn block_count(&self) -> usize;

    fn kind(&self) -> DiskKind;

    fn read(&self, cmds: &mut dyn Iterator<Item = &ReadCommand<'_>>) -> Result<(), Error>;
    fn write(&self, cmds: &mut dyn Iterator<Item = &WriteCommand<'_>>) -> Result<(), Error>;
}

#[derive(Debug)]
pub struct ReadCommand<'a> {
    pub block_off: usize,
    pub buff: &'a mut Buffer, // `buff.len()` should be a multiple of the disk's block size.
}

#[derive(Debug)]
pub struct WriteCommand<'a> {
    pub block_off: usize,
    pub buff: &'a Buffer, // `buff.len()` should be a multiple of the disk's block size.
}

#[derive(Debug, Clone, Copy)]
pub enum DiskKind {
    NVMe(/* namespace_id: */ usize),
}

impl DiskKind {
    fn get_name(self) -> String {
        match self {
            Self::NVMe(namespace) => {
                static COUNTER: AtomicUsize = AtomicUsize::new(0);
                let id = COUNTER.fetch_add(1, Ordering::Relaxed);
                format!("nvme{}n{}", id, namespace)
            }
        }
    }
}

pub type DiskRef = SmartPtr<DiskInfo<dyn Disk>>;

#[derive(Debug)]
pub struct DiskInfo<T: ?Sized + Disk> {
    pub disk: T,
}

impl<T: Disk> FsNode for DiskInfo<T> {
    fn size(&self) -> Result<usize, Error> {
        Ok(self.disk.block_size() * self.disk.block_count())
    }

    fn read<'a>(
        &self,
        offset: usize,
        buff: &'a mut [MaybeUninit<u8>],
    ) -> Result<&'a mut [u8], Error> {
        let block_size = self.disk.block_size();
        let read_size = buff.len();

        let last_byte = (offset + read_size) - 1;

        let start_block = offset / block_size;
        let end_block = last_byte / block_size;

        if end_block - start_block < 2 {
            let buff_size = if start_block == end_block {
                block_size
            } else {
                debug_assert_eq!(start_block + 1, end_block);
                block_size * 2
            };
            let mut tmp_buff = Buffer::new_boxed(buff_size);
            let cmd = ReadCommand {
                block_off: start_block,
                buff: &mut tmp_buff,
            };
            self.disk.read(&mut iter::once(&cmd))?;
            let r = MaybeUninit::write_slice(
                buff,
                &tmp_buff[(offset % block_size)..(offset % block_size) + read_size],
            );
            Ok(r)
        } else {
            let start_aligned = offset % block_size == 0;
            let end_aligned = (offset + read_size) % block_size == 0;
            let cmd = {
                let start_block_idx = if start_aligned {
                    start_block
                } else {
                    start_block + 1
                };
                let end_block_idx = if end_aligned {
                    end_block
                } else {
                    end_block - 1
                };
                let count = end_block_idx - start_block_idx + 1;
                let buff_start = if start_aligned {
                    0
                } else {
                    block_size - (offset % block_size)
                };
                let buff_end = buff_start + count * block_size;
                let buff = &mut buff[buff_start..buff_end];
                ReadCommand {
                    block_off: start_block_idx,
                    buff: Buffer::from_slice_mut(buff),
                }
            };

            let cmd1 = if !start_aligned {
                let buff = Buffer::new_boxed(block_size);
                let cmd = ReadCommand {
                    block_off: start_block,
                    buff: Box::leak(buff),
                };
                Some(cmd)
            } else {
                None
            };

            let cmd2 = if !end_aligned {
                let buff = Buffer::new_boxed(block_size);
                let cmd = ReadCommand {
                    block_off: end_block,
                    buff: Box::leak(buff),
                };
                Some(cmd)
            } else {
                None
            };

            self.disk.read(
                &mut [&Some(cmd), &cmd1, &cmd2]
                    .iter()
                    .filter_map(|&c| c.as_ref()),
            )?;

            if let Some(cmd) = cmd1 {
                let useful_slice = &cmd.buff[(offset % block_size)..];
                MaybeUninit::write_slice(
                    &mut buff[..block_size - (offset % block_size)],
                    useful_slice,
                );
                // Safety: `cmd.buff` was previously allocated with a box and is never reused after that.
                let boxed_buff = unsafe { Box::from_raw(cmd.buff as *mut _) };
                drop(boxed_buff); // free the buffer
            }

            if let Some(cmd) = cmd2 {
                let useful_slice = &cmd.buff[..((offset + read_size) % block_size)];
                MaybeUninit::write_slice(
                    &mut buff[read_size - ((offset + read_size) % block_size)..],
                    useful_slice,
                );
                // Safety: `cmd.buff` was previously allocated with a box and is never reused after that.
                let boxed_buff = unsafe { Box::from_raw(cmd.buff as *mut _) };
                drop(boxed_buff); // free the buffer
            }

            // Safety: we have written all the buffer
            let buff = unsafe { MaybeUninit::slice_assume_init_mut(buff) };
            Ok(buff)
        }
    }
}

static DISKS: Mutex<Vec<DiskRef>> = Mutex::new(Vec::new());

pub fn register_disk<T: Disk + 'static>(disk: T)
where
    DiskInfo<T>: FsNode,
{
    let name = disk.kind().get_name();
    let disk_info = DiskInfo { disk };

    let mut disks = DISKS.lock();
    let ptr = SmartPtr::new_boxed(disk_info);
    disks.push(ptr.clone());
    devfs::add_device(name, ptr);
}
