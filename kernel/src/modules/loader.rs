use core::{
    mem::{self},
    slice,
};

use alloc::vec::Vec;
use elf::{
    abi::{
        R_AARCH64_ABS64, R_AARCH64_CALL26, R_AARCH64_GLOB_DAT, R_AARCH64_JUMP26,
        R_AARCH64_LD_PREL_LO19, R_AARCH64_MOVW_UABS_G0_NC, R_AARCH64_MOVW_UABS_G1_NC,
        R_AARCH64_MOVW_UABS_G2_NC, R_AARCH64_MOVW_UABS_G3, SHF_ALLOC, SHT_RELA,
    },
    endian::LittleEndian,
    section::SectionHeader,
    ElfBytes,
};
use log::warn;

use crate::{
    fs::{self, ReadError},
    memory::{
        vmm::{vmm, AllocError, FindSpaceError, MemoryUsage},
        AddrSpaceSelector, VirtualAddress, PAGE_SHIFT, PAGE_SIZE,
    },
    symbols,
    utils::sizes::{SIZE_128M, SIZE_1M},
};

pub fn load(path: &str) -> Result<(), ModuleLoadError> {
    let file = fs::get_node(path).map_err(|_| ModuleLoadError::NotFound)?;

    let buff = file.read_to_end_vec(0)?;
    let loader = Loader::new(&buff)?;
    loader.load()?;

    Ok(())
}

#[derive(Debug)]
pub enum ModuleLoadError {
    NotFound,
    InvalidFileType,
    ElfParsingError(elf::ParseError),
    LoadingError(&'static str),
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
    fn from(e: elf::ParseError) -> Self {
        Self::ElfParsingError(e)
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
        let sections_iter = self
            .file
            .section_headers()
            .ok_or(ModuleLoadError::LoadingError("No sections"))?;
        let mut sections = Vec::with_capacity(sections_iter.len());
        sections.extend(sections_iter.iter());
        drop(sections_iter);

        self.load_sections(&mut sections)?;

        let (symtab, strtab) = self
            .file
            .symbol_table()?
            .ok_or(ModuleLoadError::LoadingError("No symtab"))?;

        let mut symbols = Vec::with_capacity(symtab.len());
        symbols.extend(symtab.iter());
        drop(symtab);

        let mut init = None;

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
                symbol.st_value += sections[symbol.st_shndx as usize].sh_addr as u64;
                let value = symbol.st_value as usize;
                match strtab.get(symbol.st_name as usize)? {
                    "init" => init = Some(VirtualAddress::new(value)),
                    _ => {}
                }
            } else {
                warn!(
                    "Unknown symbol section index {} for symbol {}",
                    symbol.st_shndx,
                    strtab.get(symbol.st_name as usize)?
                );
            }
        }

        for rela_section in sections
            .iter()
            .filter(|section| section.sh_type == SHT_RELA)
        {
            let relocations = self.file.section_data_as_relas(rela_section)?;
            for rela in relocations {
                let symbol = &symbols[rela.r_sym as usize];
                // the section where the relocation apply
                let section = &sections[rela_section.sh_info as usize];
                // Where to apply the relocation
                let place = VirtualAddress::new((section.sh_addr + rela.r_offset) as usize);

                // S + A
                let val = symbol.st_value.saturating_add_signed(rela.r_addend);

                let instruction = unsafe { &mut *place.as_ptr::<u32>() };

                match rela.r_type {
                    R_AARCH64_CALL26 | R_AARCH64_JUMP26 => {
                        let diff = val.wrapping_sub(place.addr() as u64);
                        if (diff as isize) < -(SIZE_128M as isize)
                            || (diff as isize) >= (SIZE_128M as isize)
                        {
                            return Err(ModuleLoadError::LoadingError("Symbol too far"));
                        }
                        assert!(diff % 4 == 0);
                        let _ = encode_immediate(instruction, 26, 0, diff as u32 >> 2, false);
                    }
                    R_AARCH64_LD_PREL_LO19 => {
                        let diff = val.wrapping_sub(place.addr() as u64);
                        if (diff as isize) < -(SIZE_1M as isize)
                            || (diff as isize) >= (SIZE_1M as isize)
                        {
                            return Err(ModuleLoadError::LoadingError("Symbol too far"));
                        }
                        assert!(diff % 4 == 0);
                        let _ = encode_immediate(instruction, 19, 5, diff as u32 >> 2, false);
                    }
                    R_AARCH64_MOVW_UABS_G0_NC => {
                        encode_immediate(instruction, 16, 5, val as u16 as u32, true)
                            .expect("Immediate doesn't fit");
                    }
                    R_AARCH64_MOVW_UABS_G1_NC => {
                        encode_immediate(instruction, 16, 5, (val >> 16) as u16 as u32, true)
                            .expect("Immediate doesn't fit");
                    }
                    R_AARCH64_MOVW_UABS_G2_NC => {
                        encode_immediate(instruction, 16, 5, (val >> 32) as u16 as u32, true)
                            .expect("Immediate doesn't fit");
                    }
                    R_AARCH64_MOVW_UABS_G3 => {
                        encode_immediate(instruction, 16, 5, (val >> 48) as u16 as u32, true)
                            .expect("Immediate doesn't fit");
                    }
                    R_AARCH64_ABS64 | R_AARCH64_GLOB_DAT => unsafe { *place.as_ptr::<u64>() = val },
                    _ => {
                        warn!("Unknown relocation type ({}) {:?}", rela.r_type, rela);
                        return Err(ModuleLoadError::LoadingError("Unknown relocation type"));
                    }
                }
            }
        }

        let init = unsafe { mem::transmute::<usize, fn() -> Result<(), ()>>(init.unwrap().addr()) };
        init().map_err(|_| ModuleLoadError::ModuleInitFailed)?;

        Ok(())
    }

    /// Load all the sections marked with `SHF_ALLOC` into module space memory
    /// Update each `sh_addr` to where the section is in memory.
    fn load_sections(&self, sections: &mut Vec<SectionHeader>) -> Result<(), ModuleLoadError> {
        let mut alloc_size = 0usize;
        for section in sections.iter() {
            if (section.sh_flags & SHF_ALLOC as u64) != 0 {
                alloc_size = alloc_size.next_multiple_of(section.sh_addralign as usize);
                alloc_size += section.sh_size as usize;
            }
        }

        let page_count = alloc_size.next_multiple_of(PAGE_SIZE) >> PAGE_SHIFT;
        let base_addr = vmm().alloc_pages(
            page_count,
            MemoryUsage::ModuleSpace,
            AddrSpaceSelector::kernel(),
        )?;

        let mut current_offset = 0usize;

        for section in sections.iter_mut() {
            if (section.sh_flags & SHF_ALLOC as u64) != 0 {
                current_offset = current_offset.next_multiple_of(section.sh_addralign as usize);

                let file_off = section.sh_offset as usize;
                let size = section.sh_size as usize;
                let file_slice = &self.data[file_off..file_off + size];

                let ptr = unsafe { base_addr.as_ptr::<u8>().add(current_offset) };
                let slice = unsafe { slice::from_raw_parts_mut(ptr, size) };
                slice.copy_from_slice(file_slice);

                section.sh_addr = (base_addr.addr() + current_offset) as u64;
                current_offset += size;
            }
        }

        Ok(())
    }
}

/// Encode an immediate of `size` bits into `instruction`.
/// Return `Err(())` if `imm_value` doesn't fit in the immediate.
fn encode_immediate(
    instruction: &mut u32,
    size: usize,
    shl: usize,
    imm_value: u32,
    check_overflow: bool,
) -> Result<(), ()> {
    let mask = ((1 << size) - 1) << shl;
    if (imm_value << shl) & !mask != 0 && check_overflow {
        return Err(());
    }
    *instruction &= !mask;
    *instruction |= (imm_value << shl) & mask;
    Ok(())
}
