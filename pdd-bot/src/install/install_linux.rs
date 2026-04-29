#![cfg(target_os = "linux")]

const SERVICE_FILE: &[u8] = include_bytes!("../../systemd/pdd.service");

fn is_we_root() -> bool {
    nix::unistd::Uid::effective().is_root()
}

pub(crate) fn install_systemd_service() -> anyhow::Result<()> {
    use std::io::Write;
    use std::path::PathBuf;
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