use core::{
    fmt::{Debug, Display},
    marker::PhantomData,
    num::NonZeroU8,
    ops::Deref,
    ptr,
};

use kernel::{
    error::Error,
    memory::{Dma, PhysicalAddress},
    scheduler::yield_now,
    sync::{no_irq_locks::NoIrqMutex, wait_map::WaitMap},
};
use spin::lock_api::Mutex;

use crate::device::Device;

#[repr(C)]
#[derive(Debug)]
pub struct SubmissionEntry {
    pub opcode: u8,
    pub flags: u8,
    pub command_id: u16,
    pub namespace_id: u32,
    pub dword2: u32,
    pub dword3: u32,
    pub metadata_ptr: u64,
    pub data_ptrs: [u64; 2],
    pub dword10: u32,
    pub dword11: u32,
    pub dword12: u32,
    pub dword13: u32,
    pub dword14: u32,
    pub dword15: u32,
}

#[derive(Debug)]
pub struct SubmissionQueue {
    pub id: SubmissionQueueId,
    pub completion_id: CompletionQueueId,
    inner: Mutex<SqInner>,
}

#[derive(Debug)]
struct SqInner {
    buff: Dma<[SubmissionEntry]>,
    tail: u16,
    head: u16,
}

impl SubmissionQueue {
    pub fn new(
        id: SubmissionQueueId,
        len: usize,
        completion_id: CompletionQueueId,
    ) -> Result<Self, Error> {
        let buff = unsafe { Dma::new_slice(len)? };
        let inner = Mutex::new(SqInner {
            buff,
            tail: 0,
            head: 0,
        });
        Ok(Self {
            id,
            completion_id,
            inner,
        })
    }

    #[inline(always)]
    pub fn is_full(&self) -> bool {
        let inner = self.inner.lock();
        let SqInner { tail, head, .. } = inner.deref();
        *head == (*tail + 1)
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        let inner = self.inner.lock();
        inner.buff.len()
    }

    #[inline(always)]
    pub fn addr(&self) -> PhysicalAddress {
        let inner = self.inner.lock();
        inner.buff.phys()
    }

    #[inline]
    fn set_head(&self, head: u16) {
        let mut inner = self.inner.lock();
        inner.head = head;
    }

    /// Return the new tail and the command id.
    pub unsafe fn submit<E: Into<SubmissionEntry>>(&self, entry: E) -> (u16, u16) {
        let entry: SubmissionEntry = entry.into();
        let id = entry.command_id;
        let mut inner = self.inner.lock();
        let tail = inner.tail;
        let ptr: *mut SubmissionEntry = &mut inner.buff[tail as usize];
        unsafe { ptr::write_volatile(ptr, entry) };
        inner.tail = (tail + 1) % inner.buff.len() as u16;
        (inner.tail, id)
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct CompletionEntry {
    command_specific: [u32; 2],
    submission_head_ptr: u16,
    submission_id: u16,
    command_id: u16,
    phase_and_status: u16,
}

impl CompletionEntry {
    #[inline(always)]
    pub fn phase(&self) -> bool {
        self.phase_and_status & 1 == 1
    }

    #[inline(always)]
    pub fn status(&self) -> Status {
        Status::from(self.phase_and_status)
    }
}

#[derive(Debug)]
pub struct CompletionQueue {
    pub id: CompletionQueueId,
    buff: Dma<[CompletionEntry]>,
    head: NoIrqMutex<u16>,
    pub interrupt_vector: Option<u16>,
    wait_map: WaitMap<u16>,
    interrupt_lock: NoIrqMutex<()>,
}

impl CompletionQueue {
    pub fn new(
        id: CompletionQueueId,
        len: usize,
        interrupt_vector: Option<u16>,
    ) -> Result<Self, Error> {
        let buff = unsafe { Dma::new_slice(len)? };
        let q = Self {
            id,
            buff,
            head: NoIrqMutex::new(0),
            interrupt_vector,
            wait_map: WaitMap::new(),
            interrupt_lock: NoIrqMutex::new(()),
        };
        Ok(q)
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.buff.len()
    }

    #[inline(always)]
    pub fn addr(&self) -> PhysicalAddress {
        self.buff.phys()
    }

    /// Return the first [CompletionEntry]. If `id` is `Some(_)`, return the entry only if `id` match.
    pub fn get(&self, device: &Device, id: Option<u16>) -> Option<CompletionEntry> {
        let head = *self.head.lock();
        let entry = unsafe {
            let ptr = (self.buff.ptr() as *const CompletionEntry).add(head as usize);
            ptr::read_volatile(ptr)
        };
        if entry.phase() {
            if let Some(id) = id
                && id != entry.command_id
            {
                return None;
            }
            let mut head = self.head.lock();
            *head = (*head + 1) % self.buff.len() as u16;
            unsafe { device.write_completion_head_doorbell(self.id, *head) };
            drop(head);

            let queue = device.get_submission_queue(SubmissionQueueId::new(entry.submission_id));
            queue.set_head(entry.submission_head_ptr);

            Some(entry)
        } else {
            None
        }
    }

    /// Wait until an entry with the given `id` is ready.
    pub fn wait_entry(&self, device: &Device, id: Option<u16>) -> CompletionEntry {
        if self.interrupt_vector.is_some() {
            loop {
                let lock = self.interrupt_lock.lock(); // prevent race condition where the interrupt handler receive the entry before we wait for
                if let Some(entry) = self.get(device, id) {
                    return entry;
                }
                if let Some(id) = id {
                    self.wait_map.wait_drop(id, lock);
                } else {
                    self.wait_map.wait_any_drop(lock);
                }
            }
        } else {
            loop {
                if let Some(entry) = self.get(device, id) {
                    return entry;
                }
                yield_now();
            }
        }
    }

    pub fn interrupt_handler(&self) {
        // TODO: It may be better to properly read the entry here and send it to the threads that are waitings instead of reading it 2 times.
        let entry = {
            let head = *self.head.lock();
            unsafe {
                let ptr = (self.buff.ptr() as *const CompletionEntry).add(head as usize);
                ptr::read_volatile(ptr)
            }
        };
        if entry.phase() {
            {
                let lock = self.interrupt_lock.lock();
                drop(lock);
            }
            self.wait_map.send(entry.command_id);
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct Status {
    pub do_not_retry: bool,
    pub more: bool,
    pub cmd_retry_delay: u8,
    pub code: StatusCodeType,
}

impl Status {
    pub fn success(self) -> bool {
        matches!(self.code, StatusCodeType::Generic(GenericStatus::Success))
    }
}

impl From<u16> for Status {
    fn from(value: u16) -> Self {
        let code_type = ((value >> 9) & 0b111) as u8;
        let code = (value >> 1) as u8;
        let code = match code_type {
            0 => StatusCodeType::Generic(GenericStatus::from(code)),
            1 => StatusCodeType::CommandSpecific(code),
            2 => StatusCodeType::MediaAndDataIntegrityError,
            3 => StatusCodeType::PathRelatedStatus,
            _ => StatusCodeType::Unknown(code_type, code),
        };
        Self {
            do_not_retry: value & 0x8000 != 0,
            more: value & 0x4000 != 0,
            cmd_retry_delay: ((value >> 12) & 0b11) as u8,
            code,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum StatusCodeType {
    Generic(GenericStatus),
    CommandSpecific(u8),
    MediaAndDataIntegrityError,
    PathRelatedStatus,
    Unknown(u8, u8),
}

#[derive(Debug, Clone, Copy)]
pub enum GenericStatus {
    Success,
    InvalidOpcode,
    InvalidField,
    CommandIdConflict,
    DataTransferError,
    AbortedPowerLoss,
    InternalError,
    AbortedRequested,
    AbortedSqDeletion,
    AbortedFailedFuse,
    AbortedMissingFuse,
    InvalidNamespace,
    SequenceError,
    InvalidSglSegmentDescriptor,
    InvalidSglDescriptorsCount,
    InvalidDataSglLength,
    InvalidMetadataSglLength,
    SglDescriptorTypeInvalid,
    InvalidControllerBuffUse,
    InvalidPrpOffset,
    AtomicWriteUnitExceeded,
    OperationDenied,
    InvalidSglOffset,
    // more but lazy
    OutOfRangeLBA,
    CapacityExceeded,
    NamespaceNotReady,
    ReservationConflict,
    FormatInProgress,
    InvalidValueSize,
    InvalidKeySize,
    KvKeyDoesntExist,
    UnrecoveredError,
    KeyExists,

    #[allow(dead_code)]
    Unknown(NonZeroU8),
}

impl From<u8> for GenericStatus {
    fn from(value: u8) -> Self {
        match value {
            0x00 => Self::Success,
            0x01 => Self::InvalidOpcode,
            0x02 => Self::InvalidField,
            0x03 => Self::CommandIdConflict,
            0x04 => Self::DataTransferError,
            0x05 => Self::AbortedPowerLoss,
            0x06 => Self::InternalError,
            0x07 => Self::AbortedRequested,
            0x08 => Self::AbortedSqDeletion,
            0x09 => Self::AbortedFailedFuse,
            0x0A => Self::AbortedMissingFuse,
            0x0B => Self::InvalidNamespace,
            0x0C => Self::SequenceError,
            0x0D => Self::InvalidSglSegmentDescriptor,
            0x0E => Self::InvalidSglDescriptorsCount,
            0x0F => Self::InvalidDataSglLength,
            0x10 => Self::InvalidMetadataSglLength,
            0x11 => Self::SglDescriptorTypeInvalid,
            0x12 => Self::InvalidControllerBuffUse,
            0x13 => Self::InvalidPrpOffset,
            0x14 => Self::AtomicWriteUnitExceeded,
            0x15 => Self::OperationDenied,
            0x16 => Self::InvalidSglOffset,

            0x80 => Self::OutOfRangeLBA,
            0x81 => Self::CapacityExceeded,
            0x82 => Self::NamespaceNotReady,
            0x83 => Self::ReservationConflict,
            0x84 => Self::FormatInProgress,
            0x85 => Self::InvalidValueSize,
            0x86 => Self::InvalidKeySize,
            0x87 => Self::KvKeyDoesntExist,
            0x88 => Self::UnrecoveredError,
            0x89 => Self::KeyExists,

            _ => Self::Unknown(unsafe { NonZeroU8::new_unchecked(value) }),
        }
    }
}

pub unsafe trait QueueMarker {
    const TYPE: &'static str;
}
unsafe impl QueueMarker for SubmissionQueue {
    const TYPE: &'static str = "Submission";
}
unsafe impl QueueMarker for CompletionQueue {
    const TYPE: &'static str = "Completion";
}

pub struct QueueId<Q: QueueMarker>(u16, PhantomData<Q>);

unsafe impl<Q: QueueMarker> Send for QueueId<Q> {}
unsafe impl<Q: QueueMarker> Sync for QueueId<Q> {}

impl<Q: QueueMarker> QueueId<Q> {
    #[inline(always)]
    pub fn new(id: u16) -> Self {
        Self(id, PhantomData)
    }

    #[inline(always)]
    pub fn admin() -> Self {
        Self(0, PhantomData)
    }

    #[inline(always)]
    pub fn get(self) -> u16 {
        self.0
    }
}

impl<Q: QueueMarker> Clone for QueueId<Q> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<Q: QueueMarker> Copy for QueueId<Q> {}
impl<Q: QueueMarker> PartialEq for QueueId<Q> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<Q: QueueMarker> Eq for QueueId<Q> {}
impl<Q: QueueMarker> PartialOrd for QueueId<Q> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<Q: QueueMarker> Ord for QueueId<Q> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl<Q: QueueMarker> Debug for QueueId<Q> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}Id({})", Q::TYPE, self.0)
    }
}

impl<Q: QueueMarker> Display for QueueId<Q> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub type SubmissionQueueId = QueueId<SubmissionQueue>;
pub type CompletionQueueId = QueueId<CompletionQueue>;
