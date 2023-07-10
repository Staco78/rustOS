use core::fmt::Debug;

use tock_registers::{
    register_bitfields, register_structs,
    registers::{ReadOnly, ReadWrite},
};

register_structs! {
    pub Registers {
        (0x00 => pub cap: ReadOnly<u64, Capabilities::Register>),
        (0x08 => pub vs: ReadOnly<u32>),
        (0x0C => pub intms: ReadWrite<u32>),
        (0x10 => pub intmc: ReadWrite<u32>),
        (0x14 => pub cc: ReadWrite<u32, Configuration::Register>),
        (0x18 => __reserved),
        (0x1C => pub csts: ReadOnly<u32, Status::Register>),
        (0x20 => pub nssr: ReadOnly<u32>),
        (0x24 => pub aqa: ReadWrite<u32>),
        (0x28 => pub asq: ReadWrite<u64>),
        (0x30 => pub acq: ReadWrite<u64>),
        (0x38 => pub cmbloc: ReadOnly<u32>),
        (0x3C => pub cmbsz: ReadOnly<u32>),
        (0x40 => pub bpinfo: ReadOnly<u32>),
        (0x44 => pub bprsel: ReadOnly<u32>),
        (0x48 => pub bpmbl: ReadOnly<u64>),
        (0x50 => pub cmbsmc: ReadOnly<u64>),
        (0x58 => pub cmbsts: ReadOnly<u32>),
        (0x5C => pub cmbebs: ReadOnly<u32>),
        (0x60 => pub cmbswtp: ReadOnly<u32>),
        (0x64 => pub nssd: ReadOnly<u32>),
        (0x68 => pub crto: ReadOnly<u32>),
        (0x6C => __reserved2),
        (0xE00 => pub pmrcap: ReadOnly<u32>),
        (0xE04 => pub pmrctl: ReadOnly<u32>),
        (0xE08 => pub pmrsts: ReadOnly<u32>),
        (0xE0C => pub pmrebs: ReadOnly<u32>),
        (0xE10 => pub pmrswtp: ReadOnly<u32>),
        (0xE14 => pub pmrmscl: ReadOnly<u32>),
        (0xE18 => pub pmrmscu: ReadOnly<u32>),

        (0xE1C => @END),
    }
}

impl Debug for Registers {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "NVMeRegisters")
    }
}

register_bitfields![
    u64,

    pub Capabilities [
        CRIMS OFFSET(60) NUMBITS(1) [],
        CRWMS OFFSET(59) NUMBITS(1) [],
        NSSS OFFSET(58) NUMBITS(1) [],
        CMBS OFFSET(57) NUMBITS(1) [],
        PMRS OFFSET(56) NUMBITS(1) [],
        MPSMAX OFFSET(52) NUMBITS(4) [],
        MPSMIN OFFSET(48) NUMBITS(4) [],
        CPS OFFSET(46) NUMBITS(2) [
            NotReported = 0b00,
            ControllerScope = 0b01,
            DomainScope = 0b10,
            NvmSubsystemScope = 0b11
        ],
        BPS OFFSET(45) NUMBITS(1) [],
        CSS OFFSET(37) NUMBITS(8) [],
        NSSRS OFFSET(36) NUMBITS(1) [],
        DSTRD OFFSET(32) NUMBITS(4) [],
        TO OFFSET(24) NUMBITS(8) [],
        AMS OFFSET(17) NUMBITS(2) [],
        CQR OFFSET(16) NUMBITS(1) [],
        MQES OFFSET(0) NUMBITS(16) [],
    ],
];

register_bitfields![
    u32,

    pub Configuration [
        CRIME OFFSET(24) NUMBITS(1) [],
        IOCQES OFFSET(20) NUMBITS(4) [],
        IOSQES OFFSET(16) NUMBITS(4) [],
        SHN OFFSET(14) NUMBITS(2) [
            NoNotification = 0b00,
            NormalShutdown = 0b01,
            AbruptShutdown = 0b10
        ],
        AMS OFFSET(11) NUMBITS(3) [
            RoundRobin = 0b000,
            WeightedRoundRobin = 0b001,
            VendorSpecific = 0b111
        ],
        MPS OFFSET(7) NUMBITS(4) [],
        CSS OFFSET(4) NUMBITS(3) [],
        EN OFFSET(0) NUMBITS(1) [],
    ],

    pub Status [
        ST OFFSET(6) NUMBITS(1) [],
        PP OFFSET(5) NUMBITS(1) [],
        NSSRO OFFSET(4) NUMBITS(1) [],
        SHST OFFSET(2) NUMBITS(2) [
            Normal = 0b00,
            ShutdownOccuring = 0b01,
            ShutdownComplete = 0b10,
        ],
        CFS OFFSET(1) NUMBITS(1) [],
        RDY OFFSET(0) NUMBITS(1) [],
    ]
];
