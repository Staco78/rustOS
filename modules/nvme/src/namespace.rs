use core::{fmt::Debug, mem::size_of};

use alloc::{sync::Arc, vec::Vec};
use kernel::{
    disks::{Disk, DiskKind, ReadCommand, WriteCommand},
    error::Error,
    memory::{Dma, PhysicalAddress, VirtualAddress, PAGE_SHIFT, PAGE_SIZE},
};
use log::warn;

use crate::{
    cmd::Command,
    device::Device,
    identify::NamespaceInfos,
    queues::{CompletionQueueId, SubmissionQueueId},
};

pub struct Namespace {
    device: Arc<Device>,
    infos: NamespaceInfos,
    sq: SubmissionQueueId,
    cq: CompletionQueueId,
}

impl Debug for Namespace {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Namespace")
            .field("infos", &self.infos)
            .field("sq", &self.sq)
            .field("cq", &self.cq)
            .finish()
    }
}

impl Namespace {
    pub fn new(device: Arc<Device>, infos: NamespaceInfos) -> Result<Self, Error> {
        let cq = device.create_completion_queue()?;
        let sq = device.create_submission_queue(cq)?;

        Ok(Self {
            device,
            infos,
            sq,
            cq,
        })
    }
}

impl Disk for Namespace {
    #[inline]
    fn block_count(&self) -> usize {
        self.infos.block_count as usize
    }
    #[inline]
    fn block_size(&self) -> usize {
        self.infos.format.data_size()
    }
    fn kind(&self) -> DiskKind {
        DiskKind::NVMe(self.infos.id as usize)
    }

    fn read(&self, cmds: &mut dyn Iterator<Item = &ReadCommand<'_>>) -> Result<(), Error> {
        // the maximum number of memory pages that a command can use at once
        let max_pages_count = 1 << self.device.controller_infos().mdts as usize;
        let mut sent_cmds = Vec::new();
        let queue = self.device.get_submission_queue(self.sq);

        for cmd in cmds {
            assert_eq!(cmd.buff.len() % self.block_size(), 0);
            let block_count = cmd.buff.len() / self.block_size();
            if cmd.block_off + block_count > self.block_count() {
                return Err(Error::CustomStr("Trying to read outside of the disk"));
            }

            // the number of memory pages that the buff is on
            let pages_count = {
                let buff_start = cmd.buff.phys().addr();
                let buff_last_byte = buff_start + cmd.buff.len() - 1;
                let first_page = buff_start / PAGE_SIZE;
                let last_page = buff_last_byte.div_ceil(PAGE_SIZE);
                last_page - first_page
            };
            debug_assert_ne!(pages_count, 0);

            // the max number of blocks to read at once depending on the max page count and buff alignement
            let max_blocks_count = {
                let off = cmd.buff.phys().addr() & ((1 << PAGE_SHIFT) - 1);
                let size = max_pages_count * PAGE_SIZE - off;
                size / self.block_size()
            };

            let mut pages_to_read = pages_count;
            let mut blocks_to_read = block_count;
            let mut block_off = cmd.block_off;
            let mut buff_addr = cmd.buff.phys();
            while pages_to_read > 0 {
                let readed_blocks = max_blocks_count.min(blocks_to_read);
                let readed_pages = pages_to_read.min(max_pages_count);

                let (dptr1, dptr2, prp_list_buff) = data_addrs(readed_pages, buff_addr)?;

                if prp_list_buff.is_some() {
                    assert_eq!(dptr2 % 4, 0);
                } else {
                    assert_eq!(dptr2 % PAGE_SIZE, 0);
                }

                let device_cmd = Command::read(
                    dptr1,
                    dptr2,
                    self.infos.id,
                    block_off as u64,
                    u16::try_from(readed_blocks - 1).expect("Too many blocks to read"),
                );

                // Safety: The device will write in `buff` which we have a mut reference over it so we're allowed to write on it
                // so it's as safe at it can be for this kind of things.
                let cmd_id = unsafe { self.device.submit_cmd(&queue, device_cmd) };
                sent_cmds.push((cmd_id, prp_list_buff));

                pages_to_read -= readed_pages;
                blocks_to_read -= readed_blocks;
                block_off += readed_blocks;
                buff_addr += readed_pages * PAGE_SIZE;
            }
        }

        let completion_id = queue.completion_id;
        debug_assert_eq!(completion_id, self.cq);
        drop(queue);
        let queue = self.device.get_completion_queue(completion_id);
        for (id, list_buff) in sent_cmds {
            let r = self.device.wait_cmd(&queue, id);
            if !r.status().success() {
                warn!("Read failed: {:?}", r.status());
                return Err(Error::CustomStr("Read failed"));
            }
            drop(list_buff);
        }
        Ok(())
    }

    fn write(&self, cmds: &mut dyn Iterator<Item = &WriteCommand<'_>>) -> Result<(), Error> {
        // the maximum number of memory pages that a command can use at once
        let max_pages_count = 1 << self.device.controller_infos().mdts as usize;
        let mut sent_cmds = Vec::new();
        let queue = self.device.get_submission_queue(self.sq);

        for cmd in cmds {
            assert_eq!(cmd.buff.len() % self.block_size(), 0);
            let block_count = cmd.buff.len() / self.block_size();
            if cmd.block_off + block_count > self.block_count() {
                return Err(Error::CustomStr("Trying to write outside of the disk"));
            }

            // the number of memory pages that the buff is on
            let pages_count = {
                let buff_start = cmd.buff.phys().addr();
                let buff_last_byte = buff_start + cmd.buff.len() - 1;
                let first_page = buff_start / PAGE_SIZE;
                let last_page = buff_last_byte.div_ceil(PAGE_SIZE);
                last_page - first_page
            };
            debug_assert_ne!(pages_count, 0);

            // the max number of blocks to write at once depending on the max page count and buff alignement
            let max_blocks_count = {
                let off = cmd.buff.phys().addr() & ((1 << PAGE_SHIFT) - 1);
                let size = max_pages_count * PAGE_SIZE - off;
                size / self.block_size()
            };

            let mut pages_to_write = pages_count;
            let mut blocks_to_write = block_count;
            let mut block_off = cmd.block_off;
            let mut buff_addr = cmd.buff.phys();
            while pages_to_write > 0 {
                let written_blocks = max_blocks_count.min(blocks_to_write);
                let written_pages = pages_to_write.min(max_pages_count);

                let (dptr1, dptr2, prp_list_buff) = data_addrs(written_pages, buff_addr)?;

                if prp_list_buff.is_some() {
                    assert_eq!(dptr2 % 4, 0);
                } else {
                    assert_eq!(dptr2 % PAGE_SIZE, 0);
                }

                let device_cmd = Command::write(
                    dptr1,
                    dptr2,
                    self.infos.id,
                    block_off as u64,
                    u16::try_from(written_blocks - 1).expect("Too many blocks to write"),
                );

                // Safety: The device will read in `buff` which is a valid reference
                // so it's as safe at it can be for this kind of things.
                let cmd_id = unsafe { self.device.submit_cmd(&queue, device_cmd) };
                sent_cmds.push((cmd_id, prp_list_buff));

                pages_to_write -= written_pages;
                blocks_to_write -= written_blocks;
                block_off += written_blocks;
                buff_addr += written_pages * PAGE_SIZE;
            }
        }

        let completion_id = queue.completion_id;
        debug_assert_eq!(completion_id, self.cq);
        drop(queue);
        let queue = self.device.get_completion_queue(completion_id);
        for (id, list_buff) in sent_cmds {
            let r = self.device.wait_cmd(&queue, id);
            if !r.status().success() {
                warn!("Write failed: {:?}", r.status());
                return Err(Error::CustomStr("Write failed"));
            }
            drop(list_buff);
        }
        Ok(())
    }
}

#[allow(clippy::type_complexity)]
fn data_addrs(
    pages_count: usize,
    buff_addr: PhysicalAddress,
) -> Result<(PhysicalAddress, PhysicalAddress, Option<Dma<[u64]>>), Error> {
    if pages_count == 1 {
        Ok((buff_addr, PhysicalAddress::new(0), None))
    } else if pages_count == 2 {
        let first = buff_addr;
        let second = (first + PAGE_SIZE).addr() & !((1 << PAGE_SHIFT) - 1);
        Ok((first, PhysicalAddress::new(second), None))
    } else {
        const ENTRIES_PER_PAGE: usize = PAGE_SIZE / size_of::<u64>();

        let in_list_pages_count = pages_count - 1;
        let lists_count = { (in_list_pages_count - 1).div_ceil(ENTRIES_PER_PAGE - 1) };

        let mut buff = unsafe { Dma::<[u64]>::new_slice(in_list_pages_count + lists_count)? };

        // the address of the second page (the first in a list)
        let mut addr: u64 = ((buff_addr + PAGE_SIZE).addr() & !((1 << PAGE_SHIFT) - 1)) as u64;

        'outer: for list in 0..lists_count {
            for i in (list * ENTRIES_PER_PAGE)..((list + 1) * ENTRIES_PER_PAGE - 1) {
                if i >= buff.len() {
                    break 'outer;
                }
                buff[i] = addr;
                addr += PAGE_SIZE as u64;
            }
            if list < lists_count - 1 {
                buff[(list + 1) * ENTRIES_PER_PAGE - 1] =
                    VirtualAddress::new(&buff[(list + 1) * ENTRIES_PER_PAGE] as *const _ as usize)
                        .to_phys()
                        .expect("Not mapped")
                        .addr() as u64;
            }
        }

        Ok((buff_addr, buff.phys(), Some(buff)))
    }
}
