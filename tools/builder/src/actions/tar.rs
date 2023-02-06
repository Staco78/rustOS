use std::{
    fs::{self, File},
    path::PathBuf,
};

use tar::Builder;

use super::{Action, ActionRef};

#[derive(Debug)]
pub struct TarCreateArchiveAction {
    name: Option<String>,
    output_file: String,
    input_dir: String,
    dependencies: Vec<ActionRef>,
}

impl TarCreateArchiveAction {
    pub fn new(
        name: Option<String>,
        output_file: String,
        input_dir: String,
        dependencies: Vec<ActionRef>,
    ) -> Self {
        Self {
            name,
            output_file,
            input_dir,
            dependencies,
        }
    }
}

impl Action for TarCreateArchiveAction {
    fn name(&self) -> Option<String> {
        self.name
            .clone()
            .or(Some(format!("Create archive {}", &self.output_file)))
    }
    fn dependencies<'a>(&'a mut self) -> &'a mut Vec<ActionRef> {
        &mut self.dependencies
    }
    fn run(self: Box<Self>) -> Result<(), Box<dyn std::error::Error>> {
        let mut archive = Builder::new(File::create(&self.output_file)?);
        let dir = fs::read_dir(&self.input_dir)?;
        for file in dir {
            let file = file.unwrap();
            assert!(file.file_type()?.is_file());
            let file_path = {
                let mut path = PathBuf::new();
                path.push(&self.input_dir);
                path.push(file.file_name());
                path
            };
            archive.append_path_with_name(file_path, file.file_name())?;
        }
        Ok(())
    }
}
