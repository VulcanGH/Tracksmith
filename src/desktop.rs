//! Native desktop integration. Kept outside the metadata engine.
use std::path::PathBuf;

#[cfg(windows)]
mod picker;
#[cfg(windows)]
mod tray;
pub fn show_error(message: &str) {
    #[cfg(windows)]
    {
        let text: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
        let title: Vec<u16> = "Tracksmith".encode_utf16().chain(Some(0)).collect();
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                title.as_ptr(),
                0x10,
            );
        }
    }
}

pub fn open_url(url: &str) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        let value: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
        let result = unsafe {
            windows_sys::Win32::UI::Shell::ShellExecuteW(
                std::ptr::null_mut(),
                std::ptr::null(),
                value.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
            )
        };
        anyhow::ensure!(result as isize > 32, "Windows could not open the interface");
    }
    Ok(())
}
pub fn start_tray(
    desktop: std::sync::Arc<crate::lifecycle::Desktop>,
    batch: std::sync::Arc<crate::batch::Batch>,
    dir: PathBuf,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        #[cfg(windows)]
        if let Err(error) = tray::run(desktop, batch, dir) {
            tracing::error!(%error,"Tray unavailable; use Settings to exit Tracksmith");
        }
    })
}
#[derive(serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Integration {
    pub startup: bool,
    pub explorer: bool,
}
const RUN_KEY: &str = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const CLASSES: &str = "HKCU\\Software\\Classes";
fn registry(args: &[&str]) -> anyhow::Result<std::process::Output> {
    let mut command = std::process::Command::new("reg.exe");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    Ok(command.args(args).output()?)
}
pub fn integration_status() -> Integration {
    Integration {
        startup: registry(&["query", RUN_KEY, "/v", "Tracksmith"])
            .is_ok_and(|o| o.status.success()),
        explorer: registry(&[
            "query",
            &format!("{CLASSES}\\SystemFileAssociations\\.mkv\\shell\\Tracksmith.Open"),
        ])
        .is_ok_and(|o| o.status.success()),
    }
}
pub fn configure_integration(input: Integration) -> anyhow::Result<Integration> {
    let executable = std::env::current_exe()?;
    let exe = executable.display().to_string();
    anyhow::ensure!(!exe.contains('"'), "Unsupported executable path");
    let set = |key: &str, name: &str, value: &str| -> anyhow::Result<()> {
        let output = registry(&["add", key, "/v", name, "/t", "REG_SZ", "/d", value, "/f"])?;
        anyhow::ensure!(
            output.status.success(),
            "Windows registry update failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    };
    // /ve has no value-name argument; keep the native argument list explicit.
    let default = |key: &str, value: &str| -> anyhow::Result<()> {
        let output = registry(&["add", key, "/ve", "/t", "REG_SZ", "/d", value, "/f"])?;
        anyhow::ensure!(
            output.status.success(),
            "Explorer registration failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    };
    if input.startup {
        set(RUN_KEY, "Tracksmith", &format!("\"{exe}\" --background"))?;
    } else if integration_status().startup {
        let output = registry(&["delete", RUN_KEY, "/v", "Tracksmith", "/f"])?;
        anyhow::ensure!(output.status.success(), "Could not remove startup entry");
    }
    for suffix in [
        "SystemFileAssociations\\.mkv",
        "SystemFileAssociations\\.mka",
        "Directory",
        "Directory\\Background",
    ] {
        let key = format!("{CLASSES}\\{suffix}\\shell\\Tracksmith.Open");
        if input.explorer {
            default(&key, "Open in Tracksmith")?;
            set(&key, "Icon", &format!("\"{exe}\",0"))?;
            default(
                &format!("{key}\\command"),
                &format!(
                    "\"{exe}\" -- \"{}\"",
                    if suffix.ends_with("Background") {
                        "%V"
                    } else {
                        "%1"
                    }
                ),
            )?;
        } else if registry(&["query", &key])?.status.success() {
            let output = registry(&["delete", &key, "/f"])?;
            anyhow::ensure!(output.status.success(), "Could not remove Explorer entry");
        }
    }
    Ok(integration_status())
}

pub fn valid_marker(marker: &str) -> bool {
    marker.len() == 32 && marker.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn pick_media_file(marker: &str) -> anyhow::Result<Option<PathBuf>> {
    anyhow::ensure!(
        valid_marker(marker),
        "Click Open file in the Tracksmith browser tab"
    );
    #[cfg(windows)]
    {
        Ok(picker::pick_file(marker, false)?)
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("The native file picker is available on Windows only")
    }
}

pub fn pick_media_folder(marker: &str) -> anyhow::Result<Option<PathBuf>> {
    anyhow::ensure!(
        valid_marker(marker),
        "Click Choose folder in the Tracksmith browser tab"
    );
    #[cfg(windows)]
    {
        Ok(picker::pick_file(marker, true)?)
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("The native folder picker is available on Windows only")
    }
}
