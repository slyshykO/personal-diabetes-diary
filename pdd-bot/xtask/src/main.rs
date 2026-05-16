use std::process::ExitCode;
use colorz::Colorize;
use clap::Parser;

mod args;

static CWD : std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

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
            let mut cargo_args = vec!["build"];
            if release {
                cargo_args.push("--release");
            }
            cargo(&cargo_args)
        }
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
    match cmd
        .args(args)
        .status()?
        .success()
    {
        true => Ok(()),
        false => Err(anyhow::anyhow!("[xtask] command failed")),
    }
}

fn shell(cmd: &str, args: &[&str]) -> anyhow::Result<()> {
    let shell = if cfg!(windows) {
        ("cmd.exe", "/C")
    } else {
        ("sh", "-c")
    };
    x_print_blue!("{} {}", cmd, &args.join(" "));
    let status = std::process::Command::new(shell.0)
        .arg(shell.1)
        .arg(cmd)
        .args(args)
        .status()?;

    match status.success() {
        true => Ok(()),
        false => Err(anyhow::anyhow!("[xtask] command failed: {}", cmd)),
    }
}