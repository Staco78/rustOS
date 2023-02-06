use std::{thread, time::Duration};

use super::action::{Action, ActionRef};

#[derive(Debug)]
pub struct NoopAction {
    name: Option<String>,
    dependencies: Vec<ActionRef>,
}

impl NoopAction {
    pub const fn new(name: Option<String>, dependencies: Vec<ActionRef>) -> Self {
        Self { name, dependencies }
    }
}

impl Action for NoopAction {
    fn name(&self) -> Option<String> {
        self.name.clone()
    }

    fn run(self: Box<Self>) -> Result<(), Box<dyn std::error::Error>> {
        thread::sleep(Duration::from_secs_f64(1.));
        Ok(())
    }

    fn dependencies<'a>(&'a mut self) -> &'a mut Vec<ActionRef> {
        &mut self.dependencies
    }

    fn progress_report(&self) -> bool {
        true
    }
}
