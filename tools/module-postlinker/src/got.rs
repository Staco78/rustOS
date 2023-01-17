use std::{error::Error, mem::size_of};

use object::{
    elf,
    write::{self, Object, Relocation, SectionId},
    RelocationEncoding, RelocationKind, SectionKind,
};

#[derive(Debug)]
pub struct Got {
    entries: Vec<GotEntry>,
}

impl Got {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn create_or_get(&mut self, symbol: write::SymbolId, addend: i64) -> GotEntryRef {
        let entry = self
            .entries
            .iter()
            .enumerate()
            .find(|(_, e)| e.symbol == symbol && e.addend == addend);
        if let Some(entry) = entry {
            GotEntryRef(entry.0)
        } else {
            self.entries.push(GotEntry { symbol, addend });
            GotEntryRef(self.entries.len() - 1)
        }
    }

    pub fn write(self, object: &mut Object) -> Result<SectionId, Box<dyn Error>> {
        let section = object.add_section(
            Vec::new(),
            ".got".as_bytes().into(),
            SectionKind::ReadOnlyData,
        );
        object.set_section_data(
            section,
            vec![0u8; self.entries.len() * size_of::<u64>()],
            size_of::<u64>() as u64,
        );
        for (i, entry) in self.entries.iter().enumerate() {
            let relocation = Relocation {
                addend: entry.addend,
                encoding: RelocationEncoding::Generic,
                kind: RelocationKind::Elf(elf::R_AARCH64_GLOB_DAT),
                offset: (i * size_of::<u64>()) as u64,
                size: size_of::<u64>() as u8,
                symbol: entry.symbol,
            };
            object.add_relocation(section, relocation)?;
        }

        Ok(section)
    }
}

#[derive(Debug)]
pub struct GotEntry {
    symbol: write::SymbolId,
    addend: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GotEntryRef(usize);

impl GotEntryRef {
    #[inline]
    pub fn get_offset(self) -> usize {
        self.0 * size_of::<u64>()
    }
}
