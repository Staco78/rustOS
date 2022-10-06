#[derive(Debug, Clone, Copy)]
pub struct RegInfos {
    pub offset: usize,
    pub readable: bool,
    pub writable: bool,
}

impl RegInfos {
    #[inline]
    pub fn new(offset: usize, readable: bool, writable: bool) -> Self {
        Self {
            offset,
            readable,
            writable,
        }
    }
}

#[allow(non_camel_case_types, unused)]
#[derive(Debug, Clone, Copy)]
pub enum GICD {
    CTLR,
    TYPER,
    IIDR,
    TYPER2,
    STATUSR,
    SETSPI_NSR,
    CLRSPI_NSR,
    SETSPI_SR,
    CLRSPI_SR,

    IGROUPR(u8),
    ISENABLER(u8),
    ICENABLER(u8),
    ISPENDR(u8),
    ICPENDR(u8),
    ISACTIVER(u8),
    ICACTIVER(u8),
    IPRIORITYR(u8),
    ITARGETSR(u8),
    ICFGR(u8),
    IGRPMODR(u8),
    NSACR(u8),
    SGIR,
    ICPIDR2,
    CPENDSGIR(u8),
    SPENDSGIR(u8),
    IROUTER(u8),

    PIDR2,
}

impl GICD {
    pub fn infos(self) -> RegInfos {
        match self {
            GICD::CTLR => RegInfos::new(0, true, true),
            GICD::TYPER => RegInfos::new(0x4, true, false),
            GICD::IIDR => RegInfos::new(0x8, true, false),
            GICD::TYPER2 => RegInfos::new(0xC, true, false),
            GICD::STATUSR => RegInfos::new(0x10, true, true),
            GICD::SETSPI_NSR => RegInfos::new(0x40, false, true),
            GICD::CLRSPI_NSR => RegInfos::new(0x48, false, true),
            GICD::SETSPI_SR => RegInfos::new(0x50, false, true),
            GICD::CLRSPI_SR => RegInfos::new(0x58, false, true),
            GICD::IGROUPR(n) => {
                assert!(n < 32);
                RegInfos::new(0x80 + n as usize * 4, true, true)
            }
            GICD::ISENABLER(n) => {
                assert!(n < 32);
                RegInfos::new(0x100 + n as usize * 4, true, true)
            }
            GICD::ICENABLER(n) => {
                assert!(n < 32);
                RegInfos::new(0x180 + n as usize * 4, true, true)
            }
            GICD::ISPENDR(n) => {
                assert!(n < 32);
                RegInfos::new(0x200 + n as usize * 4, true, true)
            }
            GICD::ICPENDR(n) => {
                assert!(n < 32);
                RegInfos::new(0x280 + n as usize * 4, true, true)
            }
            GICD::ISACTIVER(n) => {
                assert!(n < 32);
                RegInfos::new(0x300 + n as usize * 4, true, true)
            }
            GICD::ICACTIVER(n) => {
                assert!(n < 32);
                RegInfos::new(0x380 + n as usize * 4, true, true)
            }
            GICD::IPRIORITYR(n) => {
                assert!(n < 32);
                RegInfos::new(0x400 + n as usize * 4, true, true)
            }
            GICD::ITARGETSR(n) if n < 8 => RegInfos::new(0x800 + n as usize * 4, true, false),
            GICD::ITARGETSR(n) => {
                assert!(n >= 8 && n < 32);
                RegInfos::new(0x800 + n as usize * 4, true, true)
            }
            GICD::ICFGR(n) => {
                assert!(n < 32);
                RegInfos::new(0xC00 + n as usize * 4, true, true)
            }
            GICD::IGRPMODR(n) => {
                assert!(n < 32);
                RegInfos::new(0xD00 + n as usize * 4, true, true)
            }
            GICD::NSACR(n) => {
                assert!(n < 32);
                RegInfos::new(0xE00 + n as usize * 4, true, true)
            }
            GICD::SGIR => RegInfos::new(0xF00, false, true),
            GICD::ICPIDR2 => RegInfos::new(0xF08, true, false),
            GICD::CPENDSGIR(n) => {
                assert!(n < 32);
                RegInfos::new(0xF10 + n as usize * 4, true, true)
            }
            GICD::SPENDSGIR(n) => {
                assert!(n < 32);
                RegInfos::new(0xF20 + n as usize * 4, true, true)
            }
            GICD::IROUTER(n) => {
                assert!(n < 32);
                RegInfos::new(0x6100 + n as usize * 4, true, true)
            }
            GICD::PIDR2 => RegInfos::new(0xFEF8, true, false),
        }
    }
}

#[allow(non_camel_case_types, unused)]
#[derive(Debug, Clone, Copy)]
pub enum GICC {
    CTLR,
    PMR,
    BPR,
    IAR,
    EOIR,
    RPR,
    HPPIR,
    ABPR,
    AIAR,
    AEOIR,
    AHPPIR,
    STATUSR,
    APR(u8),
    NSAPR(u8),

    IIDR,
    DIR,
}

impl GICC {
    pub fn infos(self) -> RegInfos {
        match self {
            GICC::CTLR => RegInfos::new(0, true, true),
            GICC::PMR => RegInfos::new(0x4, true, true),
            GICC::BPR => RegInfos::new(0x8, true, true),
            GICC::IAR => RegInfos::new(0xC, true, false),
            GICC::EOIR => RegInfos::new(0x10, false, true),
            GICC::RPR => RegInfos::new(0x14, true, false),
            GICC::HPPIR => RegInfos::new(0x18, true, false),
            GICC::ABPR => RegInfos::new(0x1C, true, true),
            GICC::AIAR => RegInfos::new(0x20, true, false),
            GICC::AEOIR => RegInfos::new(0x24, false, true),
            GICC::AHPPIR => RegInfos::new(0x28, true, false),
            GICC::STATUSR => RegInfos::new(0x2C, true, true),
            GICC::APR(n) => {
                assert!(n <= 3);
                RegInfos::new(0xD0 + n as usize * 4, true, true)
            }
            GICC::NSAPR(n) => {
                assert!(n <= 3);
                RegInfos::new(0xE0 + n as usize * 4, true, true)
            }
            GICC::IIDR => RegInfos::new(0xFC, true, false),
            GICC::DIR => RegInfos::new(0x1000, false, true),
        }
    }
}
