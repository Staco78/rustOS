use core::{
    fmt::Debug,
    sync::atomic::{AtomicUsize, Ordering},
};

use alloc::{format, string::String, sync::Arc};
use kernel::{
    error::Error,
    fs::block::{BlockDev, BlockDevInfos, BlockIndex},
    memory::PhysicalAddress,
    utils::buffer::Buffer,
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

    block_infos: BlockDevInfos,
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

        let block_infos = BlockDevInfos {
            block_count: infos.block_count as usize,
            block_size: infos.format.data_size(),
            name: Self::fetch_name(infos.id),
        };

        Ok(Self {
            device,
            infos,
            sq,
            cq,
            block_infos,
        })
    }

    fn fetch_name(namespace_id: u32) -> String {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("nvme{}n{}", id, namespace_id)
    }
}

impl BlockDev for Namespace {
    fn infos(&self) -> &BlockDevInfos {
        &self.block_infos
    }

    fn read(&self, block: BlockIndex, buff: &mut Buffer) -> Result<(), Error> {
        let squeue = self.device.get_submission_queue(self.sq);
        let cqueue = self.device.get_completion_queue(self.cq);
        let cmd = Command::read(
            buff.phys(),
            PhysicalAddress::new(0),
            self.infos.id,
            block.0 as u64,
            0,
        );
        let cmd_id = unsafe { self.device.submit_cmd(&squeue, cmd) };
        let r = self.device.wait_cmd(&cqueue, cmd_id);
        if r.status().success() {
            Ok(())
        } else {
            warn!("Reading block {} failed: {:?}", block.0, r);
            Err(Error::IoError)
        }
    }

    fn write(&self, block: BlockIndex, buff: &Buffer) -> Result<(), Error> {
        let squeue = self.device.get_submission_queue(self.sq);
        let cqueue = self.device.get_completion_queue(self.cq);
        let cmd = Command::write(
            buff.phys(),
            PhysicalAddress::new(0),
            self.infos.id,
            block.0 as u64,
            0,
        );
        let cmd_id = unsafe { self.device.submit_cmd(&squeue, cmd) };
        let r = self.device.wait_cmd(&cqueue, cmd_id);
        if r.status().success() {
            Ok(())
        } else {
            warn!("Writing block {} failed: {:?}", block.0, r);
            Err(Error::IoError)
        }
    }
}
