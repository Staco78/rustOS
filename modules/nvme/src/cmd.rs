use core::sync::atomic::{AtomicU16, Ordering};

use kernel::memory::PhysicalAddress;

use crate::queues::SubmissionEntry;

#[repr(u8)]
#[allow(unused)]
pub enum AdminCmdOpcode {
    DeleteSubmissionQueue = 0x00,
    CreateSubmissionQueue,
    GetLogPage,
    DeleteCompletionQueue = 0x04,
    CreateCompletionQueue,
    Identify,
    Abort = 0x08,
    // a lot more
}

#[repr(u8)]
#[allow(unused)]
pub enum IoCmdOpcode {
    Flush = 0x00,
    Write,
    Read,
    // more
}

#[derive(Debug)]
pub struct Command {
    opcode: u8,
    namespace_id: u32,
    dword2: u32,
    dword3: u32,
    metadata_ptr: u64,
    data_ptrs: [u64; 2],
    dword10: u32,
    dword11: u32,
    dword12: u32,
    dword13: u32,
    dword14: u32,
    dword15: u32,
}

impl Command {
    #[inline]
    fn new(
        opcode: u8,
        namespace_id: u32,
        dwords: [u32; 8],
        metadata_ptr: u64,
        data_ptrs: [u64; 2],
    ) -> Self {
        Self {
            opcode,
            namespace_id,
            dword2: dwords[0],
            dword3: dwords[1],
            metadata_ptr,
            data_ptrs,
            dword10: dwords[2],
            dword11: dwords[3],
            dword12: dwords[4],
            dword13: dwords[5],
            dword14: dwords[6],
            dword15: dwords[7],
        }
    }

    pub fn identify_controller(buff: PhysicalAddress) -> Self {
        Self::new(
            AdminCmdOpcode::Identify as u8,
            0,
            [0, 0, 1, 0, 0, 0, 0, 0],
            0,
            [buff.addr() as u64, 0],
        )
    }

    pub fn identify_namespace_list(buff: PhysicalAddress) -> Self {
        Self::new(
            AdminCmdOpcode::Identify as u8,
            0,
            [0, 0, 2, 0, 0, 0, 0, 0],
            0,
            [buff.addr() as u64, 0],
        )
    }

    pub fn identify_namespace(buff: PhysicalAddress, namespace_id: u32) -> Self {
        Self::new(
            AdminCmdOpcode::Identify as u8,
            namespace_id,
            [0, 0, 0, 0, 0, 0, 0, 0],
            0,
            [buff.addr() as u64, 0],
        )
    }

    pub fn create_io_completion(
        id: u16,
        buff_addr: PhysicalAddress,
        buff_len: u16,
        interrupt_vector: Option<u16>,
    ) -> Self {
        let dword10 = id as u32 | ((buff_len as u32) << 16);
        let dword11 = if let Some(vector) = interrupt_vector {
            0b11 | ((vector as u32) << 16) // Contigous with interrupts
        } else {
            0b1 // Contigous
        };
        Self::new(
            AdminCmdOpcode::CreateCompletionQueue as u8,
            0,
            [0, 0, dword10, dword11, 0, 0, 0, 0],
            0,
            [buff_addr.addr() as u64, 0],
        )
    }

    pub fn create_io_submission(
        id: u16,
        completion_id: u16,
        buff_addr: PhysicalAddress,
        buff_len: u16,
    ) -> Self {
        let dword10 = id as u32 | ((buff_len as u32) << 16);
        let dword11 = 0b1 | ((completion_id as u32) << 16);
        Self::new(
            AdminCmdOpcode::CreateSubmissionQueue as u8,
            0,
            [0, 0, dword10, dword11, 0, 0, 0, 0],
            0,
            [buff_addr.addr() as u64, 0],
        )
    }

    pub fn read(
        dptr1: PhysicalAddress,
        dptr2: PhysicalAddress,
        namespace_id: u32,
        lba: u64,
        len: u16,
    ) -> Self {
        Self {
            opcode: IoCmdOpcode::Read as u8,
            namespace_id,
            dword2: 0,
            dword3: 0,
            metadata_ptr: 0,
            data_ptrs: [dptr1.addr() as u64, dptr2.addr() as u64],
            dword10: lba as u32,
            dword11: (lba >> 32) as u32,
            dword12: len as u32,
            dword13: 0,
            dword14: 0,
            dword15: 0,
        }
    }

    pub fn write(
        dptr1: PhysicalAddress,
        dptr2: PhysicalAddress,
        namespace_id: u32,
        lba: u64,
        len: u16,
    ) -> Self {
        Self {
            opcode: IoCmdOpcode::Write as u8,
            namespace_id,
            dword2: 0,
            dword3: 0,
            metadata_ptr: 0,
            data_ptrs: [dptr1.addr() as u64, dptr2.addr() as u64],
            dword10: lba as u32,
            dword11: (lba >> 32) as u32,
            dword12: len as u32,
            dword13: 0,
            dword14: 0,
            dword15: 0,
        }
    }
}

#[allow(clippy::from_over_into)]
impl Into<SubmissionEntry> for Command {
    fn into(self) -> SubmissionEntry {
        static ID_COUNTER: AtomicU16 = AtomicU16::new(0);
        let command_id = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        SubmissionEntry {
            opcode: self.opcode,
            flags: 0,
            command_id,
            namespace_id: self.namespace_id,
            dword2: self.dword2,
            dword3: self.dword3,
            metadata_ptr: self.metadata_ptr,
            data_ptrs: self.data_ptrs,
            dword10: self.dword10,
            dword11: self.dword11,
            dword12: self.dword12,
            dword13: self.dword13,
            dword14: self.dword14,
            dword15: self.dword15,
        }
    }
}
