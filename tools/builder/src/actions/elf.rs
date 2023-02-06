use std::{ffi::CString, fs, io::Write};

use object::{File, Object, ObjectSymbol};

use super::{Action, ActionRef};

#[derive(Debug)]
pub struct SymbolsExtractAction {
    name: String,
    in_file: String,
    out_file: String,
    dependencies: Vec<ActionRef>,
}

impl SymbolsExtractAction {
    pub fn new(
        name: String,
        in_file: String,
        out_file: String,
        dependencies: Vec<ActionRef>,
    ) -> Self {
        Self {
            name,
            in_file,
            out_file,
            dependencies,
        }
    }
}

impl Action for SymbolsExtractAction {
    fn name(&self) -> Option<String> {
        Some(self.name.clone())
    }
    fn dependencies<'a>(&'a mut self) -> &'a mut Vec<ActionRef> {
        &mut self.dependencies
    }
    fn run(self: Box<Self>) -> Result<(), Box<dyn std::error::Error>> {
        let in_file = fs::read(&self.in_file)?;
        let in_file = File::parse(&*in_file)?;
        let mut out_file = fs::File::create(&self.out_file)?;

        for symbol in in_file.symbols() {
            if symbol.is_global() && symbol.is_definition() {
                out_file.write(&symbol.address().to_le_bytes())?; // write address
                let name = CString::new(symbol.name()?)?;
                out_file.write(name.as_bytes_with_nul())?; // write name
            }
        }

        Ok(())
    }
}
