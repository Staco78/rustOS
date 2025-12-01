use std::{error::Error, fs};

use object::{
    elf,
    write::{self, SectionId, SymbolId},
    Architecture, BinaryFormat, Object, ObjectSection, ObjectSymbol, RelocationKind,
    RelocationTarget, SectionIndex, SectionKind, SymbolFlags, SymbolIndex, SymbolSection,
};

use crate::plt::{self, Plt};

pub fn make_obj(in_file: &str, out_file: &str) -> Result<(), Box<dyn Error>> {
    let file = fs::read(in_file)?;
    let file = object::File::parse(&*file)?;
    assert!(file.architecture() == Architecture::Aarch64);
    assert!(file.is_64());
    assert!(file.is_little_endian());
    assert!(file.format() == BinaryFormat::Elf);

    let mut object = write::Object::new(
        BinaryFormat::Elf,
        Architecture::Aarch64,
        object::Endianness::Little,
    );

    {
        let mut sections_map: Vec<(SectionIndex, SectionId)> = Vec::new();
        for section in file.sections() {
            let section_id = match section.kind() {
                SectionKind::Text | SectionKind::Data | SectionKind::ReadOnlyData => {
                    let section_id = object.add_section(
                        Vec::new(),
                        section.name_bytes()?.into(),
                        section.kind(),
                    );
                    let new_section = object.section_mut(section_id);
                    new_section.set_data(section.data()?, section.align());
                    section_id
                }
                SectionKind::UninitializedData => {
                    let section_id = object.add_section(
                        Vec::new(),
                        section.name_bytes()?.into(),
                        section.kind(),
                    );
                    let new_section = object.section_mut(section_id);
                    new_section.append_bss(section.size(), section.align());
                    section_id
                }
                SectionKind::Metadata | SectionKind::OtherString | SectionKind::Other => continue,
                _ => panic!("Unknown section kind {:#?} {:?}", section, section.kind()),
            };
            sections_map.push((section.index(), section_id));
        }

        let get_out_section_from_in = |in_section: SectionIndex| -> Option<SectionId> {
            sections_map
                .iter()
                .find(|(id, _)| *id == in_section)
                .map(|(_, id)| *id)
        };

        let mut symbols_map: Vec<(SymbolIndex, SymbolId)> = Vec::new();
        for symbol in file.symbols() {
            let section = match symbol.section() {
                SymbolSection::Absolute => write::SymbolSection::Absolute,
                SymbolSection::Common => write::SymbolSection::Common,
                SymbolSection::Undefined => write::SymbolSection::Undefined,
                SymbolSection::Unknown | SymbolSection::None => write::SymbolSection::None,
                SymbolSection::Section(section_id) => {
                    let section = get_out_section_from_in(section_id);
                    if let Some(section) = section {
                        write::SymbolSection::Section(section)
                    } else {
                        continue;
                    }
                }
                _ => unimplemented!(),
            };
            let id = object.add_symbol(write::Symbol {
                name: symbol.name_bytes()?.into(),
                value: symbol.address(),
                size: symbol.size(),
                kind: symbol.kind(),
                scope: symbol.scope(),
                weak: symbol.is_weak(),
                section,
                flags: SymbolFlags::None,
            });
            symbols_map.push((symbol.index(), id));
        }

        let get_out_symbol_from_in = |in_symbol: SymbolIndex| -> Option<SymbolId> {
            symbols_map
                .iter()
                .find(|(id, _)| *id == in_symbol)
                .map(|(_, id)| *id)
        };

        let mut plt = Plt::new();

        for section in file.sections() {
            for relocation in section.relocations() {
                let symbol_id = match relocation.1.target() {
                    RelocationTarget::Absolute => unimplemented!(),
                    RelocationTarget::Symbol(sym) => get_out_symbol_from_in(sym).unwrap(),
                    RelocationTarget::Section(_) => unimplemented!(),
                    _ => unimplemented!(),
                };
                match relocation.1.kind() {
                    RelocationKind::PltRelative | RelocationKind::Elf(elf::R_AARCH64_JUMP26) => {
                        let symbol = object.symbol(symbol_id);

                        if symbol.is_undefined() {
                            plt.add_needed_entry(
                                symbol_id,
                                relocation.1.addend(),
                                plt::LaterRelocInfos {
                                    offset: relocation.0,
                                    size: relocation.1.size(),
                                    kind: relocation.1.kind(),
                                    encoding: relocation.1.encoding(),
                                    section: get_out_section_from_in(section.index()).unwrap(),
                                },
                            );
                        } else {
                            object.add_relocation(
                                get_out_section_from_in(section.index()).unwrap(),
                                write::Relocation {
                                    offset: relocation.0,
                                    size: relocation.1.size(),
                                    kind: relocation.1.kind(),
                                    encoding: relocation.1.encoding(),
                                    symbol: symbol_id,
                                    addend: relocation.1.addend(),
                                },
                            )?;
                        }
                    }
                    _ => {
                        let relocation = write::Relocation {
                            addend: relocation.1.addend(),
                            encoding: relocation.1.encoding(),
                            kind: relocation.1.kind(),
                            offset: relocation.0,
                            size: relocation.1.size(),
                            symbol: symbol_id,
                        };
                        object.add_relocation(
                            get_out_section_from_in(section.index()).unwrap(),
                            relocation,
                        )?;
                    }
                }
            }
        }

        plt.write(&mut object)?;
    }

    object.write_stream(
        fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(out_file)?,
    )?;

    Ok(())
}
