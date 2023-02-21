use core::fmt::Debug;

use alloc::string::String;
use thiserror::Error;

#[derive(Error, Clone)]
pub enum Error {
    #[error("{0}")]
    Custom(String),

    #[error("{0}")]
    CustomStr(&'static str),

    #[error("Fs error: {0}")]
    Fs(FsError),

    #[error("Module load error: {0}")]
    ModuleLoad(ModuleLoadError),

    #[error("Memory error: {0}")]
    Memory(MemoryError),

    #[error("IO error")]
    IoError,
}

impl Debug for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self)
    }
}

#[derive(Error, Debug, Clone)]
pub enum FsError {
    #[error("{0}")]
    Custom(String),

    #[error("{0}")]
    CustomStr(&'static str),

    #[error("Not found")]
    NotFound,

    #[error("{0} isn't implemented")]
    NotImplemented(&'static str),

    #[error("Invalid filesystem")]
    InvalidFS,
}

#[derive(Error, Debug, Clone)]
pub enum ModuleLoadError {
    #[error("Elf parsing error")]
    ElfParsingError,
    #[error("{0}")]
    LoadingError(&'static str),
    #[error("Kernel symbol not found: {0}")]
    KernelSymbolNotFound(String),
    #[error("Missing module symbol {0}")]
    MissingModuleSymbol(&'static str),
    #[error("Module init failed: {0}")]
    ModuleInitFailed(String),
}

#[derive(Error, Debug, Clone)]
pub enum MemoryError {
    #[error("Out of memory")]
    OutOfPhysicalMemory,
    #[error("Out of virtual space")]
    OutOfVirtualSpace,
    #[error("Invalid address space")]
    InvalidAddrSpace,
    #[error("Page already mapped")]
    AlreadyMapped,
    #[error("Trying to unmap a non-mapped page")]
    NotMapped,
}
