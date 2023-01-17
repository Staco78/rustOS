use std::error::Error;

use object::{
    elf,
    write::{self, Object, Relocation, SectionId},
    RelocationEncoding, RelocationKind, SectionKind,
};

use crate::got::{Got, GotEntryRef};

#[derive(Debug)]
pub struct Plt {
    got: Got,
    entries: Vec<PltEntry>,
}

impl Plt {
    pub fn new() -> Self {
        Self {
            got: Got::new(),
            entries: Vec::new(),
        }
    }

    /// Add an plt entry only if it not duplicate.
    pub fn add_needed_entry(
        &mut self,
        symbol: write::SymbolId,
        addend: i64,
        reloc_infos: LaterRelocInfos,
    ) {
        let got_entry = self.got.create_or_get(symbol, addend);
        let entry = self.entries.iter_mut().find(|e| e.got_entry == got_entry);
        if let Some(entry) = entry {
            entry.in_reloc.push(reloc_infos);
        } else {
            self.entries.push(PltEntry {
                got_entry,
                in_reloc: vec![reloc_infos],
            });
        }
    }

    pub fn write(self, object: &mut Object) -> Result<(), Box<dyn Error>> {
        let got = self.got.write(object)?;
        let got_symbol = object.section_symbol(got);
        let section = object.add_section(Vec::new(), ".plt".as_bytes().into(), SectionKind::Text);
        let plt_symbol = object.section_symbol(section);

        let mut data: Vec<u32> = Vec::new();
        const ENTRY_SIZE: usize = 8;
        for (i, entry) in self.entries.iter().enumerate() {
            data.push(0x58000010); // ldr x16, #value
                                   // value in imm 19 (19 bits << 5) * 4
            object.add_relocation(
                section,
                Relocation {
                    offset: (i * ENTRY_SIZE) as u64,
                    symbol: got_symbol,
                    addend: entry.got_entry.get_offset() as i64,
                    encoding: RelocationEncoding::Generic,
                    kind: RelocationKind::Elf(elf::R_AARCH64_LD_PREL_LO19),
                    size: 4,
                },
            )?;

            data.push(0xD61F0200); // br x16

            for rela in entry.in_reloc.iter() {
                let relocation = Relocation {
                    encoding: rela.encoding,
                    kind: rela.kind,
                    symbol: plt_symbol,
                    offset: rela.offset,
                    size: rela.size,
                    addend: (i * ENTRY_SIZE).try_into().unwrap(),
                };
                object.add_relocation(rela.section, relocation)?;
            }
        }

        let data: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();
        object.set_section_data(section, data, 4);

        Ok(())
    }
}

#[derive(Debug)]
pub struct PltEntry {
    got_entry: GotEntryRef,
    in_reloc: Vec<LaterRelocInfos>, // where and which relocations link to this entry
}

#[derive(Debug)]
pub struct LaterRelocInfos {
    pub offset: u64,
    pub size: u8,
    pub kind: RelocationKind,
    pub encoding: RelocationEncoding,
    pub section: SectionId,
}
