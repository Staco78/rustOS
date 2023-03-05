use std::{
    fs,
    io::ErrorKind,
    path::PathBuf,
    process::Command,
    str::FromStr,
    sync::Mutex,
    thread::{self, JoinHandle},
};

use crate::error::Error;

use super::{Action, ActionRef};

#[derive(Debug)]
pub struct CommandAction {
    cmd: Command,
    name: Option<String>,
    dependencies: Vec<ActionRef>,
    progress_report: bool,
}

impl CommandAction {
    pub fn new(
        cmd: Command,
        name: Option<String>,
        progress_report: bool,
        dependencies: Vec<ActionRef>,
    ) -> Self {
        Self {
            cmd,
            name,
            dependencies,
            progress_report,
        }
    }
}

impl Action for CommandAction {
    fn name(&self) -> Option<String> {
        self.name
            .clone()
            .or(Some(format!("{}", format_cmd(&self.cmd))))
    }

    fn dependencies<'a>(&'a mut self) -> &'a mut Vec<ActionRef> {
        &mut self.dependencies
    }

    fn run(mut self: Box<Self>) -> Result<(), Box<dyn std::error::Error>> {
        let r = self.cmd.status();
        if let Err(e) = r {
            return if e.kind() == ErrorKind::NotFound {
                Err(Box::new(Error(format!(
                    "Command {:?} not found",
                    self.cmd.get_program()
                ))))
            } else {
                Err(Box::new(e))
            };
        }
        let r = r.unwrap();
        if r.success() {
            Ok(())
        } else {
            Err(Box::new(Error(format!(
                "{} returned status code {}",
                format_cmd(&self.cmd),
                r.code().unwrap_or(255)
            ))))
        }
    }
    fn progress_report(&self) -> bool {
        self.progress_report
    }
}

pub fn format_cmd(cmd: &Command) -> String {
    cmd.get_args()
        .fold(cmd.get_program().to_string_lossy().to_string(), |a, b| {
            a + " " + b.to_str().unwrap()
        })
}

#[derive(Debug)]
pub struct KernelRelinkAction {
    dependencies: Vec<ActionRef>,
}

impl KernelRelinkAction {
    pub fn new(dependencies: Vec<ActionRef>) -> Self {
        Self { dependencies }
    }
}

impl Action for KernelRelinkAction {
    fn name(&self) -> Option<String> {
        Some("Relink kernel".into())
    }
    fn dependencies<'a>(&'a mut self) -> &'a mut Vec<ActionRef> {
        &mut self.dependencies
    }
    fn run(self: Box<Self>) -> Result<(), Box<dyn std::error::Error>> {
        let mut args: Vec<String> = fs::read_dir("build/kernel_objs")
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .map(|p| {
                let mut path = PathBuf::from_str("build/kernel_objs").unwrap();
                path.push(p);
                path.to_string_lossy().to_string()
            })
            .collect();
        args.push("-obuild/kernel".into());
        args.push("-Tlinker.ld".into());
        args.push("-x".into());
        let mut cmd = Command::new("aarch64-linux-gnu-ld");
        cmd.args(&args);

        Box::new(CommandAction::new(cmd, None, true, Vec::new())).run()
    }
}

#[derive(Debug)]
pub struct BackgroundCommandAction {
    cmd: Option<Command>,
    name: Option<String>,
    dependencies: Vec<ActionRef>,
}

impl BackgroundCommandAction {
    pub fn new(cmd: Command, name: Option<String>, dependencies: Vec<ActionRef>) -> Self {
        Self {
            cmd: Some(cmd),
            name,
            dependencies,
        }
    }
}

impl Action for BackgroundCommandAction {
    fn name(&self) -> Option<String> {
        self.name
            .clone()
            .or(Some(format!("{}", format_cmd(self.cmd.as_ref().unwrap()))))
    }

    fn dependencies<'a>(&'a mut self) -> &'a mut Vec<ActionRef> {
        &mut self.dependencies
    }

    fn run(mut self: Box<Self>) -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = self.cmd.take().unwrap();
        let thread = thread::spawn(move || {
            let _ = cmd.status();
        });
        JOINS.lock()?.push(thread);
        Ok(())
    }
}

static JOINS: Mutex<Vec<JoinHandle<()>>> = Mutex::new(Vec::new());

pub fn wait_commands() {
    let mut joins = JOINS.lock().unwrap();
    for join in joins.drain(..) {
        let _ = join.join();
    }
}
