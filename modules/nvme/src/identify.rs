use alloc::vec::Vec;
use kernel::{error::Error, memory::Dma};
use static_assertions::assert_eq_size;

use crate::{cmd::Command, device::Device, queues::SubmissionQueueId};

#[repr(C)]
#[derive(Debug, Clone)]
pub struct IndentifyControllerData {
    pub vid: u16,
    pub ssvid: u16,
    pub sn: [u8; 20],
    pub mn: [u8; 40],
    pub fr: [u8; 8],
    pub rab: u8,
    pub ieee: [u8; 3],
    pub cmic: u8,
    pub mdts: u8,
    pub cntlid: u16,
    pub ver: u32,
    pub rtd3r: u32,
    pub rtd3e: u32,
    pub oaes: u32,
    pub elbas: u32,
    pub rrls: u16,
    __reserved1: [u8; 9],
    pub cntrltype: u8,
    pub fguid: [u8; 16],
    pub crdt1: u16,
    pub crdt2: u16,
    pub crdt3: u16,
    __reserved2: [u8; 106],
    __reserved3: [u8; 13],
    pub nvmsr: u8,
    pub vwci: u8,
    pub mec: u8,
    pub oacs: u16,
    pub acl: u8,
    pub aerl: u8,
    pub frmw: u8,
    pub lpa: u8,
    pub elpe: u8,
    pub npss: u8,
    pub avscc: u8,
    pub apsta: u8,
    pub wctemp: u16,
    pub cctemp: u16,
    pub mtfa: u16,
    pub hmpre: u32,
    pub hmmin: u32,
    pub tnvmcap: [u8; 16],
    // more
}

#[repr(C)]
#[derive(Debug)]
pub struct IdentifyNamespaceData {
    nsze: u64,
    ncap: u64,
    nusze: u64,
    nsfeat: u8,
    nlbaf: u8,
    flbas: u8,
    mc: u8,
    dpc: u8,
    dps: u8,
    nmic: u8,
    rescap: u8,
    fpi: u8,
    dlfeat: u8,
    nawum: u16,
    nawupf: u16,
    nacwu: u16,
    nabsn: u16,
    nabo: u16,
    nabspf: u16,
    noiob: u16,
    nvmcap: u128,
    npwg: u16,
    npwa: u16,
    npdg: u16,
    npda: u16,
    nows: u16,
    mssrl: u16,
    mcl: u16,
    msrc: u8,
    __reserved: [u8; 11],
    anagrpid: u16,
    __reserved2: [u8; 3],
    nsattr: u8,
    nvmsetid: u16,
    endgid: u16,
    nguid: u128,
    eui64: u64,
    lba_formats: [LbaFormat; 64],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LbaFormat {
    metadata_size: u16,
    lba_data_size: u8,
    relative_performance: u8,
}

impl LbaFormat {
    #[inline]
    pub fn data_size(&self) -> usize {
        1 << self.lba_data_size as usize
    }
}

assert_eq_size!(LbaFormat, u32);

#[derive(Debug, Clone, Copy)]
pub struct NamespaceInfos {
    pub id: u32,
    pub block_count: u64,
    pub format: LbaFormat,
}

impl Device {
    pub fn identify_controller(&self) -> Result<(), Error> {
        let buff: Dma<IndentifyControllerData> = unsafe { Dma::new()? };
        let cmd = Command::identify_controller(buff.phys());
        let r = unsafe { self.submit_and_wait_cmd(SubmissionQueueId::admin(), cmd) };
        assert!(r.status().success());

        unsafe { self.controller_infos.set(buff.clone()) }.unwrap();

        Ok(())
    }

    #[inline]
    pub fn controller_infos(&self) -> &IndentifyControllerData {
        self.controller_infos.get().unwrap()
    }

    pub fn identify_namespace_list(&self) -> Result<Vec<u32>, Error> {
        let buff: Dma<[u32; 1024]> = unsafe { Dma::new()? };
        let cmd = Command::identify_namespace_list(buff.phys());
        let r = unsafe { self.submit_and_wait_cmd(SubmissionQueueId::admin(), cmd) };
        assert!(r.status().success());

        Ok(buff.iter().copied().take_while(|&id| id != 0).collect())
    }

    pub fn identify_namespace(&self, namespace: u32) -> Result<NamespaceInfos, Error> {
        let buff: Dma<IdentifyNamespaceData> = unsafe { Dma::new()? };
        let cmd = Command::identify_namespace(buff.phys(), namespace);
        let r = unsafe { self.submit_and_wait_cmd(SubmissionQueueId::admin(), cmd) };
        assert!(r.status().success());

        let lba_index = (buff.flbas & 0b1101111) as usize;

        let infos = NamespaceInfos {
            id: namespace,
            block_count: buff.nsze,
            format: buff.lba_formats[lba_index],
        };

        Ok(infos)
    }
}
