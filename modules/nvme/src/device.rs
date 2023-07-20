use core::{
    hint,
    ops::Deref,
    ptr, slice,
    sync::atomic::{AtomicU16, Ordering},
};

use alloc::{boxed::Box, vec::Vec};
use alloc::{sync::Arc, vec};
use hashbrown::HashMap;
use kernel::{
    bus::pcie::{Capability, MsixCapability, MsixTableEntry, PciDevice},
    error::Error,
    interrupts::{self, InterruptMode, MsiVector},
    memory::{
        vmm::{vmm, MapFlags},
        AddrSpaceSelector, MemoryUsage, PAGE_SIZE,
    },
    utils::sync_once_cell::SyncOnceCell,
};
use log::trace;
use spin::lock_api::{Mutex, RwLock, RwLockReadGuard};
use tock_registers::interfaces::{ReadWriteable, Readable, Writeable};

use crate::{
    cmd::Command,
    identify::IndentifyControllerData,
    namespace::Namespace,
    queues::{
        CompletionEntry, CompletionQueue, CompletionQueueId, SubmissionQueue, SubmissionQueueId,
    },
    regs::{Capabilities, Configuration, Registers, Status},
    set_interrupt_handler,
};

#[derive(Debug)]
pub struct Device {
    pub regs: &'static Registers,
    pub submission_queues: RwLock<Vec<SubmissionQueue>>,
    pub completion_queues: RwLock<Vec<CompletionQueue>>,
    pub doorbell_stride: usize,
    pub msix_table: Mutex<&'static mut [MsixTableEntry]>,
    pub interrupts_map: RwLock<HashMap<u32, CompletionQueueId>>,
    pub controller_infos: SyncOnceCell<IndentifyControllerData>,
}

unsafe impl Send for Device {}
unsafe impl Sync for Device {}

impl Device {
    pub fn new(pci_device: PciDevice) -> Self {
        let bar0 = pci_device.bars().next().expect("NVMe device has not BAR0");
        let bar0_paddr = bar0.addr;
        let page_count = bar0.size.div_ceil(PAGE_SIZE);
        let bar0_vaddr = unsafe {
            vmm()
                .find_and_map(
                    bar0_paddr,
                    page_count,
                    MemoryUsage::KernelData,
                    MapFlags::new(false, false, 0, 2, false),
                    AddrSpaceSelector::kernel(),
                )
                .expect("Failed to map nvme BAR0 memory")
        };

        let ptr = bar0_vaddr.as_ptr();
        let regs: &Registers = unsafe { &*ptr };

        let dstrd = regs.cap.read(Capabilities::DSTRD);
        let doorbell_stride = 4 << dstrd;

        // Disabling controller
        regs.cc.modify(Configuration::EN::CLEAR);

        while regs.csts.is_set(Status::RDY) {
            hint::spin_loop();
        }

        trace!("Device down");

        let msix_table = {
            // Enable MSI-X interrupts
            let msix_capability = pci_device.capabilities().and_then(|mut caps| {
                caps.find_map(|c| match c {
                    Capability::Msix(msix) => Some(msix),
                    _ => None,
                })
            });
            let msix_capability = msix_capability.expect("Device doesn't support MSI-X interrupts");
            #[allow(clippy::cast_ref_to_mut)]
            unsafe {
                (*(msix_capability as *const _ as *mut MsixCapability)).enable()
            };
            let (bar, off) = msix_capability.table();
            assert_eq!(bar, 0);
            let ptr = (bar0_vaddr + off).as_ptr();
            let table: &mut [MsixTableEntry] =
                unsafe { slice::from_raw_parts_mut(ptr, msix_capability.table_len()) };
            table
        };

        let completion = CompletionQueue::new(CompletionQueueId::new(0), 32, None).unwrap();
        let submission =
            SubmissionQueue::new(SubmissionQueueId::new(0), 32, completion.id).unwrap();
        regs.aqa
            .set((completion.len() as u32 - 1) << 16 | (submission.len() as u32 - 1));

        regs.asq.set(submission.addr().addr() as u64);
        regs.acq.set(completion.addr().addr() as u64);

        let mut cc = regs.cc.extract();
        cc.modify(Configuration::IOCQES.val(4));
        cc.modify(Configuration::IOSQES.val(6));
        cc.modify(Configuration::EN::SET);
        // TODO: select page size to reflect host's page size
        regs.cc.set(cc.get());

        while !regs.csts.is_set(Status::RDY) {
            hint::spin_loop();
        }

        trace!("Device up");

        Self {
            regs,
            submission_queues: RwLock::new(vec![submission]),
            completion_queues: RwLock::new(vec![completion]),
            doorbell_stride,
            msix_table: Mutex::new(msix_table),
            interrupts_map: RwLock::new(HashMap::new()),
            controller_infos: SyncOnceCell::new(),
        }
    }

    pub fn init(self: &Arc<Self>) -> Result<(), Error> {
        self.identify_controller()?;
        let namespaces = self.identify_namespace_list()?;
        for namespace in namespaces {
            let infos = self.identify_namespace(namespace)?;
            let namespace = Namespace::new(Arc::clone(self), infos)?;
            kernel::fs::block::register_device(Box::new(namespace));
        }
        Ok(())
    }

    pub fn interrupt_handler(&self, interrupt_id: u32) {
        let map = self.interrupts_map.read();
        let queue_id = *map
            .get(&interrupt_id)
            .expect("Interrupt that no one is waiting for");
        let queue = self.get_completion_queue(queue_id);
        queue.interrupt_handler();
    }

    /// Return the interrupt and MSI-X vector if succeed.
    fn add_interrupt(&self) -> Option<(u32, u32)> {
        let MsiVector {
            addr,
            data,
            interrupt,
        } = interrupts::msi_chip().get_free_vector()?;

        let mut table = self.msix_table.lock();
        let (entry_idx, entry) = table
            .iter_mut()
            .enumerate()
            .find(|(_, entry)| entry.addr == 0)?;

        entry.addr = addr;
        entry.data = data;
        entry.unmask();

        drop(table);

        let chip = interrupts::chip();
        chip.set_mode(interrupt, InterruptMode::EdgeTriggered);
        chip.enable_interrupt(interrupt);

        set_interrupt_handler(interrupt, self);

        Some((interrupt, entry_idx as u32))
    }

    fn set_interrupt_masked(&self, msix_vector: u32, masked: bool) {
        let mut table = self.msix_table.lock();
        let entry = &mut table[msix_vector as usize];
        if masked {
            entry.mask();
        } else {
            entry.unmask();
        }
    }
    #[inline]
    fn unmask_interrupt(&self, msix_vector: u32) {
        self.set_interrupt_masked(msix_vector, false);
    }
    #[inline]
    fn mask_interrupt(&self, msix_vector: u32) {
        self.set_interrupt_masked(msix_vector, true);
    }

    #[inline]
    pub fn get_submission_queue(
        &self,
        id: SubmissionQueueId,
    ) -> impl Deref<Target = SubmissionQueue> + '_ {
        let queues = self.submission_queues.read();
        RwLockReadGuard::map(queues, |q| &q[id.get() as usize])
    }

    #[inline]
    pub fn get_completion_queue(
        &self,
        id: CompletionQueueId,
    ) -> impl Deref<Target = CompletionQueue> + '_ {
        let queues = self.completion_queues.read();
        RwLockReadGuard::map(queues, |q| &q[id.get() as usize])
    }

    #[inline]
    pub fn get_doorbell_ptr(&self, qid: u16, is_completion: bool) -> *mut u32 {
        let addend = if is_completion { 1 } else { 0 };
        ((self.regs as *const _ as usize)
            + 0x1000
            + (2 * qid as usize + addend) * self.doorbell_stride) as *mut u32
    }

    pub unsafe fn write_submission_tail_doorbell(&self, id: SubmissionQueueId, tail: u16) {
        let ptr = self.get_doorbell_ptr(id.get(), false);
        ptr::write_volatile(ptr, tail as u32);
    }

    pub unsafe fn write_completion_head_doorbell(&self, id: CompletionQueueId, head: u16) {
        let ptr = self.get_doorbell_ptr(id.get(), true);
        ptr::write_volatile(ptr, head as u32);
    }

    /// Safety: the device could write in memory.
    pub unsafe fn submit_and_wait_cmd(
        &self,
        queue_id: SubmissionQueueId,
        cmd: Command,
    ) -> CompletionEntry {
        let queue = self.get_submission_queue(queue_id);
        let cmd_id = self.submit_cmd(&queue, cmd);

        let completion_id = queue.completion_id;
        drop(queue);
        let queue = self.get_completion_queue(completion_id);
        self.wait_cmd(&queue, cmd_id)
    }

    /// Return the command_id
    ///
    /// Safety: the device could write in memory.
    pub unsafe fn submit_cmd(&self, queue: &SubmissionQueue, cmd: Command) -> u16 {
        trace!("Send command {:?} in queue {}", cmd, queue.id.get());
        if queue.is_full() {
            todo!()
        }

        let (tail, command_id) = queue.submit(cmd);
        self.write_submission_tail_doorbell(queue.id, tail);
        command_id
    }

    #[inline]
    pub fn wait_cmd(&self, queue: &CompletionQueue, command_id: u16) -> CompletionEntry {
        let response = queue.wait_entry(self, Some(command_id));
        trace!(
            "Receive response {:?} for cmd {} in queue {}",
            response,
            command_id,
            queue.id.get()
        );
        response
    }

    pub fn create_submission_queue(
        &self,
        completion_id: CompletionQueueId,
    ) -> Result<SubmissionQueueId, Error> {
        static ID_COUNTER: AtomicU16 = AtomicU16::new(1);
        let id = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        debug_assert_ne!(id, 0);

        let queue = SubmissionQueue::new(SubmissionQueueId::new(id), 32, completion_id)?;
        let cmd = Command::create_io_submission(
            id,
            completion_id.get(),
            queue.addr(),
            queue.len() as u16,
        );
        let result = unsafe { self.submit_and_wait_cmd(SubmissionQueueId::admin(), cmd) };
        assert!(result.status().success()); // TODO: return an error instead
        let id = queue.id;

        // FIXME: the queue could not be at the index of its id.
        self.submission_queues.write().push(queue);

        Ok(id)
    }

    pub fn create_completion_queue(&self) -> Result<CompletionQueueId, Error> {
        static ID_COUNTER: AtomicU16 = AtomicU16::new(1);
        let id = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        debug_assert_ne!(id, 0);

        let (int_nb, interrupt_vector) = self
            .add_interrupt()
            .ok_or(Error::CustomStr("No more free interrupts left"))?;

        let queue_id = CompletionQueueId::new(id);

        {
            let mut interrupts_map = self.interrupts_map.write();
            interrupts_map.insert(int_nb, queue_id);
            self.mask_interrupt(interrupt_vector);
        }

        let queue = CompletionQueue::new(queue_id, 32, Some(interrupt_vector as u16))?;

        let cmd = Command::create_io_completion(
            id,
            queue.addr(),
            queue.len() as u16,
            Some(interrupt_vector as u16),
        );
        let result = unsafe { self.submit_and_wait_cmd(SubmissionQueueId::admin(), cmd) };
        assert!(result.status().success()); // TODO: return an error instead
        let id = queue.id;

        // FIXME: the queue could not be at the index of its id.
        self.completion_queues.write().push(queue);

        self.unmask_interrupt(interrupt_vector);

        Ok(id)
    }
}
