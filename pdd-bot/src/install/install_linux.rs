#![cfg(target_os = "linux")]

use anyhow::Context;
use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const SERVICE_FILE: &[u8] = include_bytes!("../../systemd/pdd.service");
const SERVICE_NAME: &str = "pdd-bot.service";
const SERVICE_PATH: &str = "/run/systemd/system/pdd-bot.service";
const BINARY_PATH: &str = "/opt/alex/bin/pdd-bot";
const CONFIG_PATH: &str = "/opt/alex/share/config.toml";

#[derive(Debug)]
struct ServiceState {
    registered: bool,
    active: bool,
    load_state: String,
    active_state: String,
}

fn is_we_root() -> bool {
    nix::unistd::Uid::effective().is_root()
}

pub(crate) fn install_systemd_service() -> anyhow::Result<()> {
    if !is_we_root() {
        anyhow::bail!("must be run as root");
    }

    if !Path::new("/run/systemd/system").exists() {
        anyhow::bail!("systemd is not available");
    }

    let service_state = query_service_state()?;
    println!(
        "{SERVICE_NAME}: load={}, active={}",
        service_state.load_state, service_state.active_state
    );

    if service_state.active {
        println!("{SERVICE_NAME} is active; stopping it before update");
        run_systemctl(&["stop", SERVICE_NAME])?;
    }

    let service_changed = install_bytes(Path::new(SERVICE_PATH), SERVICE_FILE, 0o644)
        .with_context(|| format!("failed to write {SERVICE_PATH}"))?;
    if service_changed {
        println!("service unit written to {SERVICE_PATH}");
    } else {
        println!("service unit is already up to date at {SERVICE_PATH}");
    }

    if service_changed || !service_state.registered {
        run_systemctl(&["daemon-reload"])?;
        println!("systemd daemon reloaded");
    }

    install_current_exe()?;
    ensure_or_check_config()?;

    if service_state.active || !service_state.registered {
        run_systemctl(&["start", SERVICE_NAME])?;
        println!("{SERVICE_NAME} started");
    } else {
        println!("{SERVICE_NAME} was registered but inactive; leaving it stopped");
    }

    Ok(())
}

pub(crate) fn uninstall_systemd_service() -> anyhow::Result<()> {
    if !is_we_root() {
        anyhow::bail!("must be run as root");
    }

    if !Path::new("/run/systemd/system").exists() {
        anyhow::bail!("systemd is not available");
    }

    let service_state = query_service_state()?;
    println!(
        "{SERVICE_NAME}: load={}, active={}",
        service_state.load_state, service_state.active_state
    );

    if service_state.active {
        println!("{SERVICE_NAME} is active; stopping it before uninstall");
        run_systemctl(&["stop", SERVICE_NAME])?;
    }

    if service_state.registered {
        run_systemctl(&["disable", SERVICE_NAME])?;
        println!("{SERVICE_NAME} disabled");
    } else {
        println!("{SERVICE_NAME} is not registered");
    }

    let service_removed = remove_file_if_exists(Path::new(SERVICE_PATH))
        .with_context(|| format!("failed to remove {SERVICE_PATH}"))?;
    if service_removed {
        println!("service unit removed from {SERVICE_PATH}");
    } else {
        println!("service unit is already absent at {SERVICE_PATH}");
    }

    if service_state.registered || service_removed {
        run_systemctl(&["daemon-reload"])?;
        println!("systemd daemon reloaded");
    }

    remove_installed_file(BINARY_PATH)?;
    remove_installed_file(CONFIG_PATH)?;

    Ok(())
}

fn query_service_state() -> anyhow::Result<ServiceState> {
    let load_state = systemd_property("LoadState")?;
    let active_state = systemd_property("ActiveState")?;
    let registered = !load_state.is_empty() && load_state != "not-found";
    let active = active_state == "active";

    Ok(ServiceState {
        registered,
        active,
        load_state,
        active_state,
    })
}

fn systemd_property(property: &str) -> anyhow::Result<String> {
    let property_arg = format!("--property={property}");
    let output = systemctl_output(&["show", SERVICE_NAME, &property_arg, "--value"])?;
    if !output.status.success() {
        anyhow::bail!(
            "systemctl show {property} failed: {}",
            command_output_text(&output)
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_systemctl(args: &[&str]) -> anyhow::Result<()> {
    let output = systemctl_output(args)?;
    if !output.status.success() {
        anyhow::bail!(
            "systemctl {} failed: {}",
            args.join(" "),
            command_output_text(&output)
        );
    }

    Ok(())
}

fn systemctl_output(args: &[&str]) -> anyhow::Result<Output> {
    Command::new("systemctl")
        .args(args)
        .output()
        .with_context(|| format!("failed to run systemctl {}", args.join(" ")))
}

fn command_output_text(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = stdout.trim();
    let stderr = stderr.trim();

    match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => output.status.to_string(),
        (false, true) => stdout.to_string(),
        (true, false) => stderr.to_string(),
        (false, false) => format!("{stdout}; {stderr}"),
    }
}

fn install_current_exe() -> anyhow::Result<()> {
    let current_exe = std::env::current_exe().context("failed to resolve current executable")?;
    let binary_path = Path::new(BINARY_PATH);
    let bytes = fs_err::read(&current_exe)
        .with_context(|| format!("failed to read {}", current_exe.display()))?;

    let changed = install_bytes(binary_path, &bytes, 0o755)
        .with_context(|| format!("failed to install binary to {BINARY_PATH}"))?;

    if changed {
        println!(
            "binary copied from {} to {BINARY_PATH}",
            current_exe.display()
        );
    } else {
        println!("binary is already up to date at {BINARY_PATH}");
    }

    Ok(())
}

fn ensure_or_check_config() -> anyhow::Result<()> {
    let config_path = Path::new(CONFIG_PATH);
    if !config_path.exists() {
        let default_config = default_config_bytes()?;
        install_bytes(config_path, &default_config, 0o600)
            .with_context(|| format!("failed to create default config at {CONFIG_PATH}"))?;
        println!("default config created at {CONFIG_PATH}");
        return Ok(());
    }

    match crate::args::AppConfig::from_file(config_path) {
        Ok(config) => match config.check_compatibility() {
            Ok(warnings) => {
                println!("config {CONFIG_PATH} is compatible with this binary");
                for warning in warnings {
                    println!("config warning: {warning}");
                }
            }
            Err(error) => {
                println!("config {CONFIG_PATH} is not compatible with this binary: {error}");
            }
        },
        Err(error) => {
            println!("config {CONFIG_PATH} is not compatible with this binary: {error}");
        }
    }

    Ok(())
}

fn default_config_bytes() -> anyhow::Result<Vec<u8>> {
    let config = crate::args::AppConfig::default();
    let content = toml::to_string_pretty(&config)?;
    Ok(content.into_bytes())
}

fn install_bytes(path: &Path, bytes: &[u8], mode: u32) -> anyhow::Result<bool> {
    if matches!(fs_err::read(path), Ok(existing) if existing == bytes) {
        let current_mode = fs_err::metadata(path)?.permissions().mode() & 0o777;
        if current_mode != mode {
            fs_err::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
            return Ok(true);
        }

        return Ok(false);
    }

    let parent = path
        .parent()
        .with_context(|| format!("path has no parent: {}", path.display()))?;
    fs_err::create_dir_all(parent)?;

    let tmp_path = tmp_path_for(path)?;
    fs_err::write(&tmp_path, bytes)?;
    fs_err::set_permissions(&tmp_path, std::fs::Permissions::from_mode(mode))?;
    fs_err::rename(&tmp_path, path)?;
    Ok(true)
}

fn tmp_path_for(path: &Path) -> anyhow::Result<PathBuf> {
    let file_name = path
        .file_name()
        .and_then(OsStr::to_str)
        .with_context(|| format!("path has no file name: {}", path.display()))?;
    Ok(path.with_file_name(format!(".{file_name}.tmp-{}", std::process::id())))
}

fn remove_installed_file(path: &str) -> anyhow::Result<()> {
    if remove_file_if_exists(Path::new(path)).with_context(|| format!("failed to remove {path}"))? {
        println!("removed {path}");
    } else {
        println!("{path} is already absent");
    }

    Ok(())
}

fn remove_file_if_exists(path: &Path) -> anyhow::Result<bool> {
    match fs_err::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
