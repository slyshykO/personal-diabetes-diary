#![cfg(target_os = "linux")]

use std::io::Write;
use std::path::PathBuf;

const SERVICE_FILE: &[u8] = include_bytes!("../../systemd/pdd.service");

fn is_we_root() -> bool {
    nix::unistd::Uid::effective().is_root()
}

fn write_unit_file() -> anyhow::Result<()> {
    let mut fl = fs_err::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open("/lib/systemd/system/pdd-bot.service")?;
    fl.write_all(SERVICE_FILE)?;
    Ok(())
}

pub(crate) fn install_systemd_service() -> anyhow::Result<()> {
    if !is_we_root() {
        anyhow::bail!("must be run as root");
    }

    //check if systemd is available
    if !std::path::Path::new("/run/systemd/system").exists() {
        anyhow::bail!("systemd is not available");
    }

    //check if pdd-bot is running as a systemd service
    if std::path::Path::new("/run/systemd/system/pdd-bot.service").exists() {
        // check if the service is running
        let output = std::process::Command::new("systemctl")
            .arg("is-active")
            .arg("pdd-bot.service")
            .output()?;
        if output.status.success() {
            println!("pdd-bot is running as a systemd service");
            // stop the service
            std::process::Command::new("systemctl")
                .arg("stop")
                .arg("pdd-bot.service")
                .status()?;
        } else {
            println!("pdd-bot is already installed as a systemd service but not running");
        }
    }


    let mut service_path = PathBuf::from(std::env::var("HOME")?);
    service_path.push(".config/systemd/user/pdd-bot.service");
    if let Some(parent) = service_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::File::create(&service_path)?;
    file.write_all(SERVICE_FILE)?;
    println!("service file written to {}", service_path.display());
    Ok(())
}