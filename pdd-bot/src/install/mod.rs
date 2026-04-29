#[cfg(target_os = "linux")]
mod install_linux;

pub(crate) fn install() -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        install_linux::install_systemd_service()
    }

    #[cfg(not(target_os = "linux"))]
    {
        anyhow::bail!("install is only supported on Linux")
    }
}
