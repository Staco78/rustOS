use core::{mem::transmute, slice};

use alloc::vec::Vec;
use elf::{
    abi::{PF_W, PT_LOAD, R_AARCH64_JUMP_SLOT, R_AARCH64_RELATIVE, SHT_RELA},
    endian::LittleEndian,
    ElfBytes,
};
use log::{info, warn};
use module::Module;

use crate::{
    fs::{open, ReadError},
    memory::{
        round_to_page_size,
        vmm::{vmm, AllocError, FindSpaceError, MapFlags, MemoryUsage},
        AddrSpaceSelector, PAGE_SHIFT,
    },
    symbols,
};

pub fn load(path: &str) -> Result<(), ModuleLoadError> {
    let file = open(path).ok_or(ModuleLoadError::NotFound)?;
    let file = file.as_file().ok_or(ModuleLoadError::InvalidFileType)?;

    let buff = file.read_to_end_vec(0)?;
    let loader = Loader::new(&buff)?;
    loader.load()?;

    Ok(())
}

#[derive(Debug)]
pub enum ModuleLoadError {
    NotFound,
    InvalidFileType,
    ElfParsingError,
    IoError,
    OutOfMemory,
    KernelSymbolNotFound,
    MissingModuleSymbol(&'static str),
    ModuleInitFailed,
}

impl From<ReadError> for ModuleLoadError {
    #[inline]
    fn from(_: ReadError) -> Self {
        Self::IoError
    }
}

impl From<elf::ParseError> for ModuleLoadError {
    #[inline]
    fn from(_: elf::ParseError) -> Self {
        Self::ElfParsingError
    }
}

impl From<FindSpaceError> for ModuleLoadError {
    #[inline]
    fn from(_: FindSpaceError) -> Self {
        Self::OutOfMemory
    }
}

impl From<AllocError> for ModuleLoadError {
    #[inline]
    fn from(_: AllocError) -> Self {
        Self::OutOfMemory
    }
}

struct Loader<'a> {
    file: ElfBytes<'a, LittleEndian>,
    data: &'a [u8],
}

impl<'a> Loader<'a> {
    fn new(data: &'a [u8]) -> Result<Self, ModuleLoadError> {
        let file = ElfBytes::minimal_parse(data)?;
        Ok(Self { file, data })
    }

    fn load(&self) -> Result<(), ModuleLoadError> {
        let segments = self
            .file
            .segments()
            .ok_or(ModuleLoadError::ElfParsingError)?;

        let mut alloc_size = 0;
        for segment in segments {
            alloc_size = alloc_size.max((segment.p_vaddr + segment.p_memsz) as usize);
        }
        let page_count = round_to_page_size(alloc_size) >> PAGE_SHIFT;
        let base_addr = vmm().find_free_pages(
            page_count,
            MemoryUsage::ModuleSpace,
            AddrSpaceSelector::kernel(),
        )?;

        // load segments in memory
        for segment in segments.iter() {
            match segment.p_type {
                PT_LOAD => {
                    let file_off = segment.p_offset as usize;
                    let mem_off = segment.p_vaddr as usize;
                    let alloc_addr = base_addr + mem_off & !((1 << PAGE_SHIFT) - 1);
                    let mem_size = segment.p_memsz as usize;
                    let file_size = segment.p_filesz as usize;
                    debug_assert!(mem_size >= file_size);
                    let alloc_size = mem_size + (mem_off + base_addr - alloc_addr);
                    let page_count = round_to_page_size(alloc_size) >> PAGE_SHIFT;
                    let flags = MapFlags::default_rw(segment.p_flags & PF_W == 0);
                    vmm().alloc_pages_at_addr(
                        alloc_addr,
                        page_count,
                        flags,
                        AddrSpaceSelector::kernel(),
                    )?;
                    let buff = unsafe {
                        slice::from_raw_parts_mut((base_addr + mem_off) as *mut u8, mem_size)
                    };
                    let buff = &mut buff[..file_size];
                    buff.copy_from_slice(&self.data[file_off..file_off + file_size]);
                }
                _ => {}
            }
        }

        let mut module = None;
        let mut preinit = None;
        let mut name = None;

        let (symtab, strtab) = self
            .file
            .dynamic_symbol_table()?
            .ok_or(ModuleLoadError::ElfParsingError)?;
        let mut symbols = Vec::with_capacity(symtab.len());
        symbols.extend(symtab.iter());
        for symbol in &mut symbols {
            if symbol.is_undefined() {
                if symbol.st_name != 0 {
                    let name = strtab.get(symbol.st_name as usize)?;
                    let sym = symbols::get(name);
                    if let Some(sym) = sym {
                        symbol.st_value = sym as u64;
                    } else {
                        warn!("Loading module: symbol {} not found", name);
                        return Err(ModuleLoadError::KernelSymbolNotFound);
                    }
                }
            } else if symbol.st_shndx < 0xFF00 {
                symbol.st_value += base_addr as u64;
                let value = symbol.st_value as usize;
                match strtab.get(symbol.st_name as usize)? {
                    "MODULE" => module = Some(value),
                    "MODULE_NAME" => name = Some(value),
                    "__module_pre_init" => preinit = Some(value),
                    _ => {}
                }
            } else {
                warn!("Unknown symbol section index {}", symbol.st_shndx);
            }
        }

        for rela_section in self
            .file
            .section_headers()
            .ok_or(ModuleLoadError::ElfParsingError)?
            .iter()
            .filter(|s| s.sh_type == SHT_RELA)
        {
            let relocations = self.file.section_data_as_relas(&rela_section)?;
            for rela in relocations {
                let symbol = &symbols[rela.r_sym as usize];
                let relocation_place: *mut () = (base_addr as u64 + rela.r_offset) as *mut _;
                match rela.r_type {
                    R_AARCH64_JUMP_SLOT => {
                        // S + A
                        unsafe {
                            *(relocation_place as *mut u64) = symbol
                                .st_value
                                .checked_add_signed(rela.r_addend)
                                .expect("Overflow")
                        }
                    }
                    R_AARCH64_RELATIVE => {
                        // Delta(S) + A
                        if rela.r_sym == 0 {
                            unsafe {
                                *(relocation_place as *mut u64) = (base_addr as u64)
                                    .checked_add_signed(rela.r_addend)
                                    .expect("Overflow");
                            }
                        } else {
                            todo!()
                        }
                    }
                    _ => {
                        panic!("Unkown relocation type ({})", rela.r_type);
                    }
                }
            }
        }

        let name = unsafe {
            transmute::<usize, *const [usize; 2]>(
                name.ok_or(ModuleLoadError::MissingModuleSymbol("MODULE_NAME"))?,
            )
        };
        // FIXME: this is not safe
        let name = unsafe { transmute::<[usize; 2], &str>(*name) };

        let module = unsafe {
            transmute::<usize, *const [usize; 2]>(
                module.ok_or(ModuleLoadError::MissingModuleSymbol("MODULE"))?,
            )
        };
        // FIXME: this is not safe
        let module = unsafe { transmute::<[usize; 2], &dyn Module>(*module) };

        let preinit: unsafe fn() -> () = unsafe {
            transmute::<usize, _>(
                preinit.ok_or(ModuleLoadError::MissingModuleSymbol("__module_pre_init"))?,
            )
        };
        unsafe { preinit() };

        info!("Module {} loaded", name);
        module
            .init()
            .ok()
            .ok_or(ModuleLoadError::ModuleInitFailed)?;

        Ok(())
    }
}
