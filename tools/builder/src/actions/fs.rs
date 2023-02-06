use std::fs;

use super::{Action, ActionRef};

#[derive(Debug)]
pub struct MkdirAction {
    path: String,
    dependencies: Vec<ActionRef>,
}

impl MkdirAction {
    pub fn new(path: String, dependencies: Vec<ActionRef>) -> Self {
        Self { path, dependencies }
    }
}

impl Action for MkdirAction {
    fn name(&self) -> Option<String> {
        Some(format!("Create dir {}", self.path))
    }

    fn dependencies<'a>(&'a mut self) -> &'a mut Vec<ActionRef> {
        &mut self.dependencies
    }

    fn progress_report(&self) -> bool {
        true
    }

    fn run(self: Box<Self>) -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(&self.path)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct ClearDirAction {
    path: String,
    dependencies: Vec<ActionRef>,
}

impl ClearDirAction {
    pub fn new(path: String, dependencies: Vec<ActionRef>) -> Self {
        Self { path, dependencies }
    }
}

impl Action for ClearDirAction {
    fn name(&self) -> Option<String> {
        Some(format!("Clear dir {}", self.path))
    }

    fn dependencies<'a>(&'a mut self) -> &'a mut Vec<ActionRef> {
        &mut self.dependencies
    }

    fn progress_report(&self) -> bool {
        true
    }

    fn run(self: Box<Self>) -> Result<(), Box<dyn std::error::Error>> {
        fs::remove_dir_all(&self.path)?;
        fs::create_dir(&self.path)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct CopyFileAction {
    in_path: String,
    out_path: String,
    dependencies: Vec<ActionRef>,
}

impl CopyFileAction {
    pub fn new(in_path: String, out_path: String, dependencies: Vec<ActionRef>) -> Self {
        Self {
            in_path,
            out_path,
            dependencies,
        }
    }
}

impl Action for CopyFileAction {
    fn name(&self) -> Option<String> {
        Some(format!("Copy {} to {}", self.in_path, self.out_path))
    }

    fn dependencies<'a>(&'a mut self) -> &'a mut Vec<ActionRef> {
        &mut self.dependencies
    }

    fn run(self: Box<Self>) -> Result<(), Box<dyn std::error::Error>> {
        fs::copy(self.in_path, self.out_path)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct DeleteAction {
    path: String,
    dependencies: Vec<ActionRef>,
}

impl DeleteAction {
    pub fn new(path: String, dependencies: Vec<ActionRef>) -> Self {
        Self { path, dependencies }
    }
}

impl Action for DeleteAction {
    fn name(&self) -> Option<String> {
        Some(format!("Delete {}", self.path))
    }

    fn dependencies<'a>(&'a mut self) -> &'a mut Vec<ActionRef> {
        &mut self.dependencies
    }

    fn run(self: Box<Self>) -> Result<(), Box<dyn std::error::Error>> {
        let meta = fs::metadata(&self.path);
        if let Err(_) = meta {
            return Ok(());
        } 
        let meta = meta?;
        if meta.is_dir() {
            fs::remove_dir_all(&self.path)?;
        } else if meta.is_file() {
            fs::remove_file(&self.path)?;
        } else {
            unimplemented!()
        }
        Ok(())
    }
}
