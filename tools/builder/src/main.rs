use std::{
    env,
    error::Error,
    fmt::Display,
    path::Path,
    process::{exit, Command},
    time::Duration,
};

use actions::{
    ActionRef, BuildModuleAction, CargoCmdAction, ClearDirAction, CommandAction, CopyFileAction,
    DeleteAction, ExtractArchiveAction, FetchKernelLibsMetaAction, KernelRelinkAction, MkdirAction,
    NoopAction, SpawnCommandAction, SymbolsExtractAction, TarCreateArchiveAction,
};
use console::style;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use lazy_static::{__Deref, lazy_static};

use crate::actions::Action;

mod actions;
mod error;

lazy_static! {
    static ref SPIN_STYLE: ProgressStyle = ProgressStyle::with_template("{spinner} {wide_msg}")
        .unwrap()
        .tick_strings(&[
            "   ",
            ".  ",
            ".. ",
            "...",
            " ..",
            "  .",
            &style("✔").green().to_string(),
        ]);
}

const MODULE_LIST: &[&str] = &["hello"];

fn main() {
    let args: Vec<_> = {
        let mut args = env::args();
        args.next();
        args.collect()
    };

    let params: Vec<_> = args.iter().filter(|e| !e.starts_with("-")).collect();
    let options: Vec<_> = args.iter().filter(|e| e.starts_with("-")).collect();

    let mut release = false;

    for option in options.iter() {
        match option.as_str() {
            "-r" | "--release" => release = true,
            _ => panic!("Invalid option {}", option),
        }
    }

    let action = match params[0].as_str() {
        "build" => action_build(release),
        "run" => action_run(release),
        "clean" => action_clean(),
        "check" => action_check(),
        _ => {
            print_error_and_exit(format!("Unknown command {}", params[0]));
        }
    };

    if let Err(e) = action {
        print_error_and_exit(e);
    }

    let r = run_action(action.unwrap());
    if let Err(e) = r {
        print_error_and_exit(e);
    }
}

fn print_error_and_exit<T: Display>(msg: T) -> ! {
    println!("{}", style(format!("{}", msg)).red());
    exit(-1);
}

fn run_action(action_ref: ActionRef) -> Result<(), Box<dyn Error>> {
    let mut action_ = action_ref.get_mut();
    if action_.is_none() {
        return Ok(());
    }
    let action = action_.as_mut().unwrap().as_mut();
    for action in action.dependencies().drain(..) {
        run_action(action)?;
    }

    let action_name = action.name();
    let progress_report = action.progress_report();

    drop(action_);

    if progress_report {
        if let Some(action_name) = action_name {
            let spin = ProgressBar::new_spinner();
            spin.set_draw_target(ProgressDrawTarget::stdout());
            spin.set_style(SPIN_STYLE.deref().clone());
            spin.enable_steady_tick(Duration::from_millis(120));
            spin.set_message(style(&action_name).yellow().to_string());

            let r = action_ref.run();

            if r.is_ok() {
                spin.finish_with_message(style(action_name).green().to_string());
            }

            r
        } else {
            action_ref.run()
        }
    } else {
        let r = action_ref.run();

        if let Some(action_name) = action_name {
            println!("{}", style("✔ ".to_string() + &action_name).green());
        }

        r
    }
}

fn action_build(release: bool) -> Result<ActionRef, Box<dyn Error>> {
    let create_dirs = MkdirAction::new("build/kernel_objs".into(), vec![]).into();
    let create_dirs: ActionRef = MkdirAction::new("initrd".into(), vec![create_dirs]).into();
    let loader = CargoCmdAction::new(
        "loader/Cargo.toml",
        Some("Loader".into()),
        "build",
        release,
        "aarch64-unknown-uefi",
        &[],
        vec![],
    )
    .into();
    let loader = CopyFileAction::new(
        format!(
            "target/aarch64-unknown-uefi/{}/loader.efi",
            if release { "release" } else { "debug" }
        ),
        "build/boot.efi".into(),
        vec![loader, create_dirs.clone()],
    )
    .into();
    let kernel_build: ActionRef = CargoCmdAction::new(
        "kernel/Cargo.toml",
        Some("Kernel".into()),
        "build",
        release,
        Path::new("targets/aarch64-kernel.json")
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap(),
        &[],
        vec![],
    )
    .into();
    let clear_kernel_objs =
        ClearDirAction::new("build/kernel_objs".into(), vec![create_dirs]).into();
    let extract_kernel_objs = ExtractArchiveAction::new(
        None,
        format!(
            "target/aarch64-kernel/{}/libkernel.a",
            if release { "release" } else { "debug" }
        ),
        "build/kernel_objs/".into(),
        vec![clear_kernel_objs, kernel_build.clone()],
    )
    .into();
    let kernel: ActionRef = KernelRelinkAction::new(vec![extract_kernel_objs]).into();
    let ksymbols = SymbolsExtractAction::new(
        "Extract kernel symbols".into(),
        "build/kernel".into(),
        "initrd/ksymbols".into(),
        vec![kernel.clone()],
    )
    .into();

    let mut initrd_dependencies = vec![ksymbols];

    let fetch: ActionRef = FetchKernelLibsMetaAction::new(vec![kernel_build], release).into();

    for module in MODULE_LIST.iter().copied().map(String::from) {
        let module = BuildModuleAction::new(module, release, vec![fetch.clone()]).into();
        initrd_dependencies.push(module);
    }

    let initrd = TarCreateArchiveAction::new(
        None,
        "build/initrd.tar".into(),
        "initrd/".into(),
        initrd_dependencies,
    )
    .into();

    Ok(NoopAction::new(None, vec![loader, kernel, initrd]).into())
}

fn action_run(release: bool) -> Result<ActionRef, Box<dyn Error>> {
    let build_action = action_build(release)?;
    let mut cmd = Command::new(env::var("QEMU").unwrap_or("qemu-system-aarch64".into()));
    cmd.args(&[
        "-machine",
        "virt",
        "-cpu",
        "max",
        "-drive",
        "if=pflash,format=raw,file=QEMU_CODE.fd,readonly=on",
        "-drive",
        "if=pflash,format=raw,file=QEMU_VARS.fd",
        "-drive",
        &format!(
            "format=raw,file=fat:rw:{}",
            Path::new("root")
                .canonicalize()?
                .as_os_str()
                .to_str()
                .unwrap()
        ),
        "-net",
        "none",
        "-monitor",
        "stdio",
        "-smp",
        "4",
        "-m",
        "256",
    ]);
    Ok(SpawnCommandAction::new(cmd, Some("Run qemu".into()), vec![build_action]).into())
}

fn action_clean() -> Result<ActionRef, Box<dyn Error>> {
    let delete_build = DeleteAction::new("build".into(), vec![]).into();
    let delete_initrd = DeleteAction::new("initrd".into(), vec![]).into();
    let mut cmd = Command::new("cargo");
    cmd.args(&["clean", "--manifest-path=kernel/Cargo.toml", "-q"]);
    let clean_kernel = CommandAction::new(cmd, None, true, vec![]).into();
    let mut cmd = Command::new("cargo");
    cmd.args(&["clean", "--manifest-path=loader/Cargo.toml", "-q"]);
    let clean_loader = CommandAction::new(cmd, None, true, vec![]).into();
    Ok(NoopAction::new(
        None,
        vec![delete_build, delete_initrd, clean_kernel, clean_loader],
    )
    .into())
}

fn action_check() -> Result<ActionRef, Box<dyn Error>> {
    fn check(manifest: &str, target: &str, lib: bool) -> Result<(), Box<dyn Error>> {
        let args: &[&str] = if lib {
            &["--message-format=json-diagnostic-rendered-ansi", "--lib"]
        } else {
            &["--message-format=json-diagnostic-rendered-ansi"]
        };
        Box::new(CargoCmdAction::new(
            manifest,
            None,
            "check",
            false,
            target,
            args,
            vec![],
        ))
        .run()
    }
    check(
        "kernel/Cargo.toml",
        Path::new("targets/aarch64-kernel.json")
            .canonicalize()?
            .to_str()
            .unwrap(),
        true,
    )?;
    check("loader/Cargo.toml", "aarch64-unknown-uefi", false)?;
    for module in MODULE_LIST.iter().copied() {
        check(
            &format!("modules/{}/Cargo.toml", module),
            Path::new("targets/aarch64-kernel.json")
                .canonicalize()?
                .to_str()
                .unwrap(),
            false,
        )?;
    }

    Ok(NoopAction::new(None, vec![]).into())
}
