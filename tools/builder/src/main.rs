use std::{
    env,
    error::Error,
    fmt::Display,
    path::Path,
    process::{exit, Command},
    time::Duration,
};

use actions::{
    wait_commands, ActionRef, BackgroundCommandAction, BuildModuleAction, CargoCmdAction,
    ClearDirAction, CommandAction, CopyFileAction, DeleteAction, ExtractArchiveAction,
    FetchKernelLibsMetaAction, KernelRelinkAction, MkdirAction, NoopAction, SymbolsExtractAction,
    TarCreateArchiveAction,
};
use clap::Parser;
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

const QEMU_ARGS: &'static [&'static str] = &[
    "-machine",
    "virt",
    "-cpu",
    "max",
    "-drive",
    "if=pflash,format=raw,file=QEMU_CODE.fd,readonly=on",
    "-drive",
    "if=pflash,format=raw,file=QEMU_VARS.fd",
    "-drive",
    "format=raw,file=fat:rw:root",
    "-net",
    "none",
    "-monitor",
    "stdio",
    "-smp",
    "4",
    "-m",
    "256",
    "-drive",
    "file=initrd/ext2.disk,if=none,id=drv0,format=raw",
    "-device",
    "nvme,drive=drv0,serial=1234",
];

const MODULE_LIST: &[&str] = &["hello", "ext2", "nvme"];

#[derive(Parser, Debug)]
#[command()]
struct Args {
    action: String,

    #[arg(short, long)]
    release: bool,

    #[arg(long)]
    features: Vec<String>,

    #[arg(long)]
    no_default_features: bool,
}

fn main() {
    let args = Args::parse();

    let action = match args.action.as_str() {
        "build" => action_build(args),
        "run" => action_run(args),
        "clean" => action_clean(args),
        "check" => action_check(args),
        "debug" => action_debug(args),
        "dtb" => action_dtb(args),
        "clippy" => action_clippy(args),
        _ => {
            print_error_and_exit(format!("Unknown command {}", args.action));
        }
    };

    if let Err(e) = action {
        print_error_and_exit(e);
    }

    let r = run_action(action.unwrap());
    if let Err(e) = r {
        print_error_and_exit(e);
    }

    wait_commands();
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

fn action_build(args: Args) -> Result<ActionRef, Box<dyn Error>> {
    let create_dirs = MkdirAction::new("build/kernel_objs".into(), vec![]).into();
    let create_dirs: ActionRef = MkdirAction::new("initrd".into(), vec![create_dirs]).into();
    let loader = CargoCmdAction::new(
        "loader/Cargo.toml",
        Some("Loader".into()),
        "build",
        args.release,
        Some("aarch64-unknown-uefi"),
        &[],
        vec![],
    )
    .into();
    let loader = CopyFileAction::new(
        format!(
            "target/aarch64-unknown-uefi/{}/loader.efi",
            if args.release { "release" } else { "debug" }
        ),
        "build/boot.efi".into(),
        vec![loader, create_dirs.clone()],
    )
    .into();
    let features = format!("--features={}", args.features.join(","));
    let mut kernel_args = vec![features.as_str()];
    if args.no_default_features {
        kernel_args.push("--no-default-features");
    }
    let kernel_build: ActionRef = CargoCmdAction::new(
        "kernel/Cargo.toml",
        Some("Kernel".into()),
        "build",
        args.release,
        Some(
            Path::new("targets/aarch64-kernel.json")
                .canonicalize()
                .unwrap()
                .to_str()
                .unwrap(),
        ),
        &kernel_args,
        vec![],
    )
    .into();
    let clear_kernel_objs =
        ClearDirAction::new("build/kernel_objs".into(), vec![create_dirs]).into();
    let extract_kernel_objs = ExtractArchiveAction::new(
        None,
        format!(
            "target/aarch64-kernel/{}/libkernel.a",
            if args.release { "release" } else { "debug" }
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

    let fetch: ActionRef = FetchKernelLibsMetaAction::new(vec![kernel_build], args.release).into();

    for module in MODULE_LIST.iter().copied().map(String::from) {
        let module = BuildModuleAction::new(module, args.release, vec![fetch.clone()]).into();
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

fn action_run(args: Args) -> Result<ActionRef, Box<dyn Error>> {
    let build_action = action_build(args)?;
    let mut cmd = Command::new(env::var("QEMU").unwrap_or("qemu-system-aarch64".into()));
    cmd.args(QEMU_ARGS);
    Ok(BackgroundCommandAction::new(cmd, Some("Run qemu".into()), vec![build_action]).into())
}

fn action_clean(_args: Args) -> Result<ActionRef, Box<dyn Error>> {
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

fn action_check(_args: Args) -> Result<ActionRef, Box<dyn Error>> {
    fn check(manifest: &str, target: Option<&str>, lib: bool) -> Result<(), Box<dyn Error>> {
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
        Some(
            Path::new("targets/aarch64-kernel.json")
                .canonicalize()?
                .to_str()
                .unwrap(),
        ),
        true,
    )?;
    check("loader/Cargo.toml", Some("aarch64-unknown-uefi"), false)?;
    check("tools/builder/Cargo.toml", None, false)?;
    check("tools/module-postlinker/Cargo.toml", None, false)?;
    for module in MODULE_LIST.iter().copied() {
        check(
            &format!("modules/{}/Cargo.toml", module),
            Some(
                Path::new("targets/aarch64-kernel.json")
                    .canonicalize()?
                    .to_str()
                    .unwrap(),
            ),
            false,
        )?;
    }

    Ok(NoopAction::new(None, vec![]).into())
}

fn action_debug(args: Args) -> Result<ActionRef, Box<dyn Error>> {
    let build_action = action_build(args)?;
    let mut cmd = Command::new(env::var("QEMU").unwrap_or("qemu-system-aarch64".into()));
    cmd.args(QEMU_ARGS);
    cmd.arg("-s");
    cmd.arg("-S");
    Ok(BackgroundCommandAction::new(cmd, Some("Run qemu".into()), vec![build_action]).into())
}

fn action_dtb(_args: Args) -> Result<ActionRef, Box<dyn Error>> {
    let mut cmd = Command::new(env::var("QEMU").unwrap_or("qemu-system-aarch64".into()));
    cmd.args(QEMU_ARGS);
    cmd.arg("-machine");
    cmd.arg("dumpdtb=qemu.dtb");
    Ok(BackgroundCommandAction::new(cmd, Some("Run qemu".into()), vec![]).into())
}

fn action_clippy(args: Args) -> Result<ActionRef, Box<dyn Error>> {
    let clippy = |manifest: &str, target: &str, lib: bool| -> Result<(), Box<dyn Error>> {
        let mut cmd_args = vec!["--keep-going", "-Zunstable-options"];
        if lib {
            cmd_args.push("--lib");
        }
        Box::new(CargoCmdAction::new(
            manifest,
            None,
            "clippy",
            args.release,
            Some(target),
            &cmd_args,
            vec![],
        ))
        .run()
    };
    clippy(
        "kernel/Cargo.toml",
        Path::new("targets/aarch64-kernel.json")
            .canonicalize()?
            .to_str()
            .unwrap(),
        true,
    )?;
    for module in MODULE_LIST.iter().copied() {
        clippy(
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
