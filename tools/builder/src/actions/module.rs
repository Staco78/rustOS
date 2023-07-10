use std::{
    collections::HashMap,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
    time::SystemTime,
    vec,
};

use cargo_toml::Manifest;

use crate::actions::CommandAction;

use super::{Action, ActionRef};

static KERNEL_LIBS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

#[derive(Debug)]
pub struct BuildModuleAction {
    module_name: String,
    dependencies: Vec<ActionRef>,
    release: bool,
}

impl BuildModuleAction {
    pub fn new(name: String, release: bool, dependencies: Vec<ActionRef>) -> Self {
        Self {
            module_name: name,
            dependencies,
            release,
        }
    }
}

impl Action for BuildModuleAction {
    fn name(&self) -> Option<String> {
        Some(format!("Build {}.kmod", &self.module_name))
    }
    fn dependencies<'a>(&'a mut self) -> &'a mut Vec<ActionRef> {
        &mut self.dependencies
    }
    fn progress_report(&self) -> bool {
        false
    }

    fn run(self: Box<Self>) -> Result<(), Box<dyn Error>> {
        let cargo_toml_file = format!("modules/{}/Cargo.toml", &self.module_name);
        let manifest = Manifest::from_path(&cargo_toml_file)?;
        let package = manifest.package.as_ref().unwrap();

        assert!(package.name == self.module_name);

        let mut cmd = Command::new("rustc");
        cmd.env("CARGO_PKG_NAME", package.name());
        cmd.env(
            "CARGO_MANIFEST_DIR",
            Path::new(&cargo_toml_file).canonicalize()?,
        );
        cmd.env("CARGO_PKG_VERSION", package.version());
        cmd.env(
            "CARGO_PKG_AUTHORS",
            package
                .authors()
                .iter()
                .fold(String::new(), |a, b| format!("{a},{b}")),
        );

        cmd.args(&[
            "--crate-name",
            package.name(),
            "--edition=2021",
            &format!("modules/{}/src/lib.rs", package.name()),
            "--crate-type",
            "staticlib",
            "-C",
            &format!("opt-level={}", if self.release { 3 } else { 0 }),
            "-C",
            "embed-bitcode=no",
            "--out-dir",
            Path::new("build").canonicalize()?.to_str().unwrap(),
            "--target",
            Path::new("targets/aarch64-kernel.json")
                .canonicalize()?
                .to_str()
                .unwrap(),
            "-L",
            &format!(
                "dependency={}",
                Path::new(&format!(
                    "target/aarch64-kernel/{}/deps",
                    if self.release { "release" } else { "debug" }
                ))
                .canonicalize()?
                .to_str()
                .unwrap()
            ),
            "-L",
            &format!(
                "dependency={}",
                Path::new(&format!(
                    "target/{}/deps",
                    if self.release { "release" } else { "debug" }
                ))
                .canonicalize()?
                .to_str()
                .unwrap()
            ),
            "-Z",
            "unstable-options",
            "-C",
            "symbol-mangling-version=v0",
            "--emit=obj",
            "-Z",
            "no-link",
            "-C",
            "code-model=large",
        ]);

        if self.release {
            cmd.args(&["-C", "strip=symbols"]);
        } else {
            cmd.args(&["-C", "debuginfo=2"]);
        }

        let kernel_libs = KERNEL_LIBS.lock()?;
        let mut add_dependency = |dep: &str, noprelude: bool| -> Result<(), Box<dyn Error>> {
            let lib_name = format!("lib{}", dep);
            let file_name = kernel_libs
                .as_ref()
                .unwrap()
                .get(&lib_name)
                .expect(&format!("Kernel lib {} not found", lib_name));
            let file_path = PathBuf::from(format!(
                "target/aarch64-kernel/{}/deps/{}",
                if self.release { "release" } else { "debug" },
                file_name
            ))
            .canonicalize()?;
            let file_path = file_path.to_str().unwrap();
            cmd.args([
                "--extern",
                &format!(
                    "{}{}={}",
                    if noprelude { "noprelude:" } else { "" },
                    dep,
                    file_path
                ),
            ]);
            Ok(())
        };

        for dep in manifest.dependencies {
            add_dependency(&dep.0.replace('-', "_"), false)?;
        }
        add_dependency("core", true)?;
        add_dependency("alloc", true)?;
        add_dependency("compiler_builtins", true)?;

        Box::new(CommandAction::new(cmd, None, false, vec![])).run()?;

        let mut ld_command = Command::new("aarch64-linux-gnu-ld");
        ld_command.args(&[
            &format!("build/{}.o", package.name()),
            "-o",
            &format!("build/{}_.o", package.name()),
            "-r",
            "-x",
            "-Tmodule-linker.ld",
        ]);
        Box::new(CommandAction::new(ld_command, None, false, vec![])).run()?;

        if !self.release {
            let mut extract_debug_cmd = Command::new("aarch64-linux-gnu-objcopy");
            extract_debug_cmd.args(&[
                "--only-keep-debug",
                &format!("build/{}_.o", package.name()),
                &format!("build/{}.debug", package.name()),
            ]);
            let mut strip_cmd = Command::new("aarch64-linux-gnu-strip");
            strip_cmd.args(&[
                "-xg",
                "-R",
                ".debug_gdb_scripts",
                &format!("build/{}_.o", package.name()),
            ]);
            Box::new(CommandAction::new(extract_debug_cmd, None, false, vec![])).run()?;
            Box::new(CommandAction::new(strip_cmd, None, false, vec![])).run()?;
        }

        module_postlinker::make_obj(
            &format!("build/{}_.o", package.name()),
            &format!("initrd/{}.kmod", package.name()),
        )?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct FetchKernelLibsMetaAction {
    dependencies: Vec<ActionRef>,
    release: bool,
}

impl FetchKernelLibsMetaAction {
    pub fn new(dependencies: Vec<ActionRef>, release: bool) -> Self {
        Self {
            dependencies,
            release,
        }
    }
}

impl Action for FetchKernelLibsMetaAction {
    fn name(&self) -> Option<String> {
        None
    }
    fn dependencies<'a>(&'a mut self) -> &'a mut Vec<ActionRef> {
        &mut self.dependencies
    }
    fn progress_report(&self) -> bool {
        false
    }
    fn run(self: Box<Self>) -> Result<(), Box<dyn std::error::Error>> {
        let deps_dir = fs::read_dir(format!(
            "target/aarch64-kernel/{}/deps/",
            if self.release { "release" } else { "debug" }
        ))?;
        let files = deps_dir.filter(|f| {
            if let Ok(f) = f {
                !f.file_type().unwrap().is_dir()
                    && f.file_name().into_string().unwrap().ends_with(".rlib")
            } else {
                false
            }
        });

        let mut hashmap: HashMap<String, (String, SystemTime)> = HashMap::new();
        for file in files {
            let file = file?;
            let file_name = file.file_name();
            let file_name = file_name.to_str().unwrap();
            let lib_name = file_name.split('-').next().unwrap();
            let modified_time = file.metadata()?.modified()?;

            if let Some(entry) = hashmap.get_mut(lib_name) {
                if modified_time > entry.1 {
                    let new_entry = (file_name.into(), modified_time);
                    *entry = new_entry;
                }
            } else {
                hashmap.insert(lib_name.into(), (file_name.into(), modified_time));
            }
        }

        let mut new_hashmap = HashMap::new();
        for (key, (value, _)) in hashmap.drain() {
            let r = new_hashmap.insert(key, value);
            assert!(r.is_none());
        }

        *KERNEL_LIBS.lock()? = Some(new_hashmap);

        Ok(())
    }
}
