use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::ExitCode;

use anyhow::Context;
use clap::Parser;
use colorz::Colorize;

mod args;

static CWD: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

#[macro_export]
macro_rules! x_print_blue {
    () => {
        eprintln!("{}", "".blue().bold());
    };
    ($($arg:tt)*) => {
        eprintln!("{} {}", "[xtask]".blue().bold(), format!($($arg)*).blue().bold());
    };
}

#[macro_export]
macro_rules! x_print_red {
    () => {
        eprintln!("{}", "".red().bold());
    };
    ($($arg:tt)*) => {
        eprintln!("{} {}", "[xtask]".red().bold(), format!($($arg)*).red().bold());
    };
}

fn main() -> ExitCode {
    if let Err(e) = xtask() {
        x_print_red!("error: {e}");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn xtask() -> anyhow::Result<()> {
    init_cwd()?;
    colorz::mode::set_coloring_mode(colorz::mode::Mode::Detect);
    x_print_blue!("started in `{}`", cwd().to_string_lossy());
    let args = args::Args::parse();
    match args.action {
        Some(args::Action::Build { release }) => {
            npm(["run", "build"])?;
            let mut cargo_args = vec!["build"];
            if release {
                cargo_args.push("--release");
            }
            cargo(&cargo_args)
        }
        Some(args::Action::Npm { args }) => npm(args),
        None => {
            anyhow::bail!("No action specified");
        }
    }
}

pub fn cwd() -> &'static std::path::PathBuf {
    CWD.get().expect("CWD not set")
}

fn init_cwd() -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    CWD.set(cwd).map_err(|_| anyhow::anyhow!("CWD already set"))
}

fn cargo(args: &[&str]) -> anyhow::Result<()> {
    x_print_blue!("cargo {}", &args.join(" "));

    let mut cmd = std::process::Command::new("cargo");
    match cmd.args(args).status()?.success() {
        true => Ok(()),
        false => Err(anyhow::anyhow!("[xtask] command failed")),
    }
}

fn npm<I, S>(args: I) -> anyhow::Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args: Vec<OsString> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_os_string())
        .collect();
    let printable_args: Vec<String> = args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();

    x_print_blue!("npm {}", printable_args.join(" "));
    let mut cmd = std::process::Command::new("npm");
    cmd.args(&args);

    if let Some(path) = sanitized_path_for_npm() {
        cmd.env("PATH", path);
    }

    let status = cmd
        .status()
        .context("failed to run npm; make sure Linux npm is installed and available in PATH")?;

    match status.success() {
        true => Ok(()),
        false => Err(anyhow::anyhow!("[xtask] command failed: npm")),
    }
}

fn sanitized_path_for_npm() -> Option<OsString> {
    if !cfg!(unix) {
        return None;
    }

    let path = std::env::var_os("PATH")?;
    let sanitized_paths = std::env::split_paths(&path)
        .filter(|path| !is_windows_path_under_unix(path))
        .collect::<Vec<_>>();

    std::env::join_paths(sanitized_paths).ok()
}

fn is_windows_path_under_unix(path: &Path) -> bool {
    let path = path.to_string_lossy();
    let Some(rest) = path.strip_prefix("/mnt/") else {
        return false;
    };

    let mut chars = rest.chars();
    matches!(chars.next(), Some(drive) if drive.is_ascii_alphabetic())
        && matches!(chars.next(), Some('/') | None)
}
