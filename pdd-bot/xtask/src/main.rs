use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Context;
use clap::Parser;
use colorz::Colorize;

mod args;

static CWD: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
const DEV_CONFIG_PATH: &str = "target/xtask/dev.config.toml";
const APP_BINARY_NAME: &str = "pdd-bot";

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
        Some(args::Action::Build { release }) => build(release),
        Some(args::Action::Run { release }) => run(release),
        Some(args::Action::Dev { config }) => dev(config.as_deref()),
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

fn build(release: bool) -> anyhow::Result<()> {
    npm(["run", "build"])?;
    cargo_build(release)
}

fn run(release: bool) -> anyhow::Result<()> {
    build(release)?;

    let binary_path = app_binary_path(release);
    x_print_blue!("run {}", binary_path.display());

    let interrupted = install_ctrlc_handler()?;
    let mut cmd = Command::new(&binary_path);
    cmd.stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    prepare_child_command(&mut cmd);

    let mut child = cmd
        .spawn()
        .with_context(|| format!("failed to run `{}`", binary_path.display()))?;

    loop {
        if interrupted.load(Ordering::SeqCst) {
            x_print_blue!("stopping {}", binary_path.display());
            interrupt_child(&mut child);
            return Ok(());
        }

        if let Some(status) = child.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(anyhow::anyhow!("binary exited with status {status}"))
            };
        }

        std::thread::sleep(Duration::from_millis(200));
    }
}

fn dev(config: Option<&str>) -> anyhow::Result<()> {
    let config_path = match config {
        Some(path) => cwd().join(path),
        None => {
            let path = cwd().join(DEV_CONFIG_PATH);
            write_dev_config(&path)?;
            path
        }
    };
    let config_path = config_path
        .canonicalize()
        .with_context(|| format!("failed to resolve config path `{}`", config_path.display()))?;

    x_print_blue!("starting Rust app on http://127.0.0.1:8080");
    x_print_blue!("starting Vite dev server");
    x_print_blue!("using config `{}`", config_path.display());

    let interrupted = install_ctrlc_handler()?;

    let mut rust = spawn_cargo(["run", "--", "--config"], [config_path.as_os_str()])?;
    let mut vite = match spawn_npm(["run", "dev"]) {
        Ok(vite) => vite,
        Err(e) => {
            terminate_child(&mut rust);
            return Err(e);
        }
    };

    loop {
        if interrupted.load(Ordering::SeqCst) {
            x_print_blue!("stopping dev processes");
            interrupt_child(&mut vite);
            interrupt_child(&mut rust);
            return Ok(());
        }

        if let Some(status) = rust.try_wait()? {
            terminate_child(&mut vite);
            return if status.success() {
                Ok(())
            } else {
                Err(anyhow::anyhow!("Rust app exited with status {status}"))
            };
        }

        if let Some(status) = vite.try_wait()? {
            terminate_child(&mut rust);
            return if status.success() {
                Ok(())
            } else {
                Err(anyhow::anyhow!("Vite dev server exited with status {status}"))
            };
        }

        std::thread::sleep(Duration::from_millis(200));
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
        .context(npm_missing_message())?;

    match status.success() {
        true => Ok(()),
        false => Err(anyhow::anyhow!("[xtask] command failed: npm")),
    }
}

fn cargo_build(release: bool) -> anyhow::Result<()> {
    let mut cargo_args = vec!["build"];
    if release {
        cargo_args.push("--release");
    }
    cargo(&cargo_args)
}

fn app_binary_path(release: bool) -> PathBuf {
    let profile_dir = if release { "release" } else { "debug" };
    cwd()
        .join("target")
        .join(profile_dir)
        .join(format!("{APP_BINARY_NAME}{}", std::env::consts::EXE_SUFFIX))
}

fn spawn_cargo<'a, I, J>(prefix_args: I, suffix_args: J) -> anyhow::Result<Child>
where
    I: IntoIterator<Item = &'a str>,
    J: IntoIterator<Item = &'a OsStr>,
{
    let prefix_args = prefix_args.into_iter().collect::<Vec<_>>();
    let suffix_args = suffix_args
        .into_iter()
        .map(|arg| arg.to_os_string())
        .collect::<Vec<_>>();
    let printable_args = prefix_args
        .iter()
        .map(|arg| (*arg).to_string())
        .chain(
            suffix_args
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned()),
        )
        .collect::<Vec<_>>();

    x_print_blue!("cargo {}", printable_args.join(" "));
    let mut cmd = Command::new("cargo");
    cmd.args(&prefix_args)
        .args(&suffix_args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    prepare_child_command(&mut cmd);
    cmd.spawn().context("failed to spawn cargo")
}

fn spawn_npm<'a, I>(args: I) -> anyhow::Result<Child>
where
    I: IntoIterator<Item = &'a str>,
{
    let args = args.into_iter().collect::<Vec<_>>();

    x_print_blue!("npm {}", args.join(" "));
    let mut cmd = Command::new("npm");
    cmd.args(&args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    if let Some(path) = sanitized_path_for_npm() {
        cmd.env("PATH", path);
    }

    prepare_child_command(&mut cmd);
    cmd.spawn().context(npm_missing_message())
}

fn write_dev_config(path: &Path) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("path has no parent: {}", path.display()))?;
    fs_err::create_dir_all(parent)?;

    let config = r#"[tg_config]
tg_bot_token = ""
tg_chat_id = []
data_dir = ".data"
input_timezone = "Europe/Kyiv"
glucose_after_meal_reminder_minutes = 150
glucose_after_meal_reminder_count = 3
glucose_after_meal_reminder_interval_minutes = 3

[html_config]
enable = true
listen = "127.0.0.1:8080"
allow = []
"#;
    fs_err::write(path, config)?;
    Ok(())
}

fn terminate_child(child: &mut Child) {
    match child.try_wait() {
        Ok(Some(_)) => return,
        Ok(None) => {}
        Err(e) => {
            x_print_red!("failed to poll child process: {e}");
            return;
        }
    }

    #[cfg(unix)]
    {
        use nix::sys::signal::{Signal, killpg};
        use nix::unistd::Pid;

        let pid = Pid::from_raw(child.id() as i32);
        if let Err(e) = killpg(pid, Signal::SIGTERM) {
            x_print_red!("failed to send SIGTERM to child process group {}: {e}", child.id());
        }

        for _ in 0..10 {
            match child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                Err(e) => {
                    x_print_red!("failed to poll child process: {e}");
                    return;
                }
            }
        }

        if let Err(e) = killpg(pid, Signal::SIGKILL) {
            x_print_red!("failed to send SIGKILL to child process group {}: {e}", child.id());
        }
    }

    #[cfg(not(unix))]
    if let Err(e) = child.kill() {
        x_print_red!("failed to kill child process {}: {e}", child.id());
    }

    if let Err(e) = child.wait() {
        x_print_red!("failed to wait for child process {}: {e}", child.id());
    }
}

fn interrupt_child(child: &mut Child) {
    match child.try_wait() {
        Ok(Some(_)) => return,
        Ok(None) => {}
        Err(e) => {
            x_print_red!("failed to poll child process: {e}");
            return;
        }
    }

    #[cfg(unix)]
    {
        use nix::sys::signal::Signal;

        if send_signal_to_child_group(child, Signal::SIGINT) {
            for _ in 0..10 {
                match child.try_wait() {
                    Ok(Some(_)) => return,
                    Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                    Err(e) => {
                        x_print_red!("failed to poll child process: {e}");
                        return;
                    }
                }
            }
        }
    }

    terminate_child(child);
}

fn prepare_child_command(cmd: &mut Command) {
    cmd.current_dir(cwd());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
}

fn install_ctrlc_handler() -> anyhow::Result<Arc<AtomicBool>> {
    let interrupted = Arc::new(AtomicBool::new(false));
    ctrlc::set_handler({
        let interrupted = Arc::clone(&interrupted);
        move || {
            interrupted.store(true, Ordering::SeqCst);
        }
    })
    .context("failed to install ctrl-c handler")?;
    Ok(interrupted)
}

#[cfg(unix)]
fn send_signal_to_child_group(child: &Child, signal: nix::sys::signal::Signal) -> bool {
    use nix::sys::signal::killpg;
    use nix::unistd::Pid;

    let pid = Pid::from_raw(child.id() as i32);
    match killpg(pid, signal) {
        Ok(()) => true,
        Err(e) => {
            x_print_red!(
                "failed to send {signal:?} to child process group {}: {e}",
                child.id()
            );
            false
        }
    }
}

fn npm_missing_message() -> &'static str {
    "failed to run npm; make sure Linux npm is installed and available in PATH (Windows /mnt/* npm entries are ignored on unix)"
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
