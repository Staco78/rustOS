use std::process::Command;

use super::{Action, ActionRef, CommandAction};

#[derive(Debug)]
pub struct CargoCmdAction {
    inner: CommandAction,
}

impl CargoCmdAction {
    pub fn new(
        manifest_path: &str,
        name: Option<String>,
        cmd: &str,
        release: bool,
        target: Option<&str>,
        args: &[&str],
        dependencies: Vec<ActionRef>,
    ) -> Self {
        let mut command = Command::new("cargo");
        command.args([
            cmd,
            format!("--manifest-path={}", manifest_path).as_str(),
            "-q",
        ]);
        if let Some(target) = target {
            command.args([
                format!("--target={}", target).as_str(),
                "-Zbuild-std=core,compiler_builtins,alloc",
                "-Zbuild-std-features=compiler-builtins-mem",
            ]);
        }
        command.args(args);
        command.env("RUSTFLAGS", "-C symbol-mangling-version=v0");
        if release {
            command.arg("-r");
        }
        Self {
            inner: CommandAction::new(command, name, true, dependencies),
        }
    }
}

impl Action for CargoCmdAction {
    fn name(&self) -> Option<String> {
        self.inner.name()
    }

    fn run(self: Box<Self>) -> Result<(), Box<dyn std::error::Error>> {
        Box::new(self.inner).run()
    }

    fn dependencies<'a>(&'a mut self) -> &'a mut Vec<ActionRef> {
        self.inner.dependencies()
    }

    fn progress_report(&self) -> bool {
        true
    }
}
