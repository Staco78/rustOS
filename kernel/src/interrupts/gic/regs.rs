use tock_registers::{
    register_bitfields, register_structs,
    registers::{ReadOnly, ReadWrite, WriteOnly},
};

register_structs! {
    pub DistributorRegs {
        (0x000 => pub ctlr: ReadWrite<u32, GICD_CTLR::Register>),
        (0x004 => pub typer: ReadOnly<u32, GICD_TYPER::Register>),
        (0x008 => pub iidr: ReadOnly<u32>),
        (0x00C => _reserved),
        (0x080 => pub igroupr: [ReadWrite<u32>; 32]),
        (0x100 => pub isenabler: [ReadWrite<u32>; 32]),
        (0x180 => pub icenabler: [ReadWrite<u32>; 32]),
        (0x200 => pub ispendr: [ReadWrite<u32>; 32]),
        (0x280 => pub icpendr: [ReadWrite<u32>; 32]),
        (0x300 => pub isactiver: [ReadWrite<u32>; 32]),
        (0x380 => pub icactiver: [ReadWrite<u32>; 32]),
        (0x400 => pub ipriorityr: [ReadWrite<u32>; 255]),
        (0x7FC => _reserved1),
        (0x800 => pub itargetsr: [ReadWrite<u32>; 255]), // first 8 are read only
        (0xBFC => _reserved2),
        (0xC00 => pub icfgr: [ReadWrite<u32>; 64]),
        (0xD00 => _reserved3),
        (0xE00 => pub nsacr: [ReadWrite<u32>; 64]),
        (0xF00 => pub sgir: WriteOnly<u32, GICD_SGIR::Register>),
        (0xF04 => _reserved4),
        (0xF10 => pub cpendsgir: [ReadWrite<u32>; 4]),
        (0xF20 => pub spendsgir: [ReadWrite<u32>; 4]),
        (0xF30 => _reserved5),
        (0xFE8 => pub icpidr2: ReadOnly<u32>),
        (0xFEC => _reserved6),
        (0x1000 => @END),
    }
}

register_structs! {
    pub CpuInterfaceRegs {
        (0x0000 => pub ctlr: ReadWrite<u32, GICC_CTLR::Register>),
        (0x0004 => pub pmr: ReadWrite<u32, GICC_PMR::Register>),
        (0x0008 => pub bpr: ReadWrite<u32>),
        (0x000C => pub iar: ReadOnly<u32, GICC_IAR::Register>),
        (0x0010 => pub eoir: WriteOnly<u32, GICC_EOIR::Register>),
        (0x0014 => pub rpr: ReadOnly<u32>),
        (0x0018 => pub hppir: ReadOnly<u32>),
        (0x001C => pub abpr: ReadWrite<u32>),
        (0x0020 => pub aiar: ReadOnly<u32>),
        (0x0024 => pub aeoir: WriteOnly<u32>),
        (0x0028 => pub ahppir: ReadOnly<u32>),
        (0x002C => _reserved),
        (0x00D0 => pub apr: [ReadWrite<u32>; 4]),
        (0x00E0 => pub nsapr: [ReadWrite<u32>; 4]),
        (0x00F0 => _reserved1),
        (0x00FC => pub iidr: ReadOnly<u32>),
        (0x0100 => _reserved2),
        (0x1000 => pub dir: WriteOnly<u32>),
        (0x1004 => @END),
    }
}

register_bitfields! [u32,
    pub GICD_CTLR [
        EnableGrp1 1,
        EnableGrp0 0
    ],

    pub GICD_TYPER [
        LSPI OFFSET(11) NUMBITS(5) [],
        SecurityExt OFFSET(10) NUMBITS(1) [],
        CpuCount OFFSET(5) NUMBITS(3) [],
        ITLinesCount OFFSET(0) NUMBITS(5) [],
    ],

    pub GICD_SGIR [
        TargetListFilter OFFSET(24) NUMBITS(2) [
            Specific = 0b00,
            All = 0b01, // all interfaces except itself
            Me = 0b10
        ],
        CpuTargetList OFFSET(16) NUMBITS(8) [],
        NSATT OFFSET(15) NUMBITS(1) [],
        ID OFFSET(0) NUMBITS(4) []
    ],

    pub GICC_CTLR [
        EOImodeNS 10,
        EOImodeS 9,
        IRQBypDisGrp1 8,
        FIQBypDisGrp1 7,
        IRQBypDisGrp0 6,
        FIQBypDisGrp0 5,
        CBPR 4,
        FIQEn 3,
        AckCtl 2,
        EnableGrp1 1,
        EnableGrp0 0
    ],

    pub GICC_PMR [
        Priority OFFSET(0) NUMBITS(8) []
    ],

    pub GICC_IAR [
        CPUID OFFSET(10) NUMBITS(3) [],
        InterruptId OFFSET(0) NUMBITS(10) []
    ],

    pub GICC_EOIR [
        CPUID OFFSET(10) NUMBITS(3) [],
        InterruptId OFFSET(0) NUMBITS(10) []
    ]
];
