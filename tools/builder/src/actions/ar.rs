use std::{fs::File, io, path::PathBuf, str};

use ar::Archive;

use super::{Action, ActionRef};

#[derive(Debug)]
pub struct ExtractArchiveAction {
    name: Option<String>,
    archive_path: String,
    output_path: String,
    dependencies: Vec<ActionRef>,
}

impl ExtractArchiveAction {
    pub fn new(
        name: Option<String>,
        archive_path: String,
        output_path: String,
        dependencies: Vec<ActionRef>,
    ) -> Self {
        Self {
            name,
            archive_path,
            output_path,
            dependencies,
        }
    }
}

impl Action for ExtractArchiveAction {
    fn name(&self) -> Option<String> {
        self.name.clone().or(Some(format!(
            "Extract {} into {}",
            self.archive_path, self.output_path
        )))
    }

    fn dependencies<'a>(&'a mut self) -> &'a mut Vec<ActionRef> {
        &mut self.dependencies
    }

    fn progress_report(&self) -> bool {
        true
    }

    fn run(self: Box<Self>) -> Result<(), Box<dyn std::error::Error>> {
        let mut archive = Archive::new(File::open(&self.archive_path)?);
        while let Some(entry) = archive.next_entry() {
            let mut entry = entry.unwrap();
            let file_name = str::from_utf8(entry.header().identifier()).unwrap();
            let new_file_name = {
                let mut path = PathBuf::new();
                path.push(&self.output_path);
                path.push(file_name);
                path
            };
            let mut file = File::create(new_file_name).unwrap();
            io::copy(&mut entry, &mut file).unwrap();
        }

        Ok(())
    }
}
