//! GitHub Releases checks and a separate Windows worker for verified replacement/restart.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};
use utoipa::ToSchema;

const SCRIPT: &str = include_str!("../packaging/update.ps1");
pub const REPOSITORY: &str = match option_env!("TRACKSMITH_UPDATE_REPOSITORY") {
    Some(value) => value,
    None => "VulcanGH/Tracksmith",
};
#[derive(Clone, Serialize, ToSchema)]
pub struct UpdateStatus {
    pub current: String,
    pub repository: String,
    pub phase: String,
    pub kind: String,
    pub available: Option<String>,
    pub notes: String,
    pub release_url: Option<String>,
    pub size: Option<u64>,
    pub checked_at: Option<u64>,
    pub message: String,
    pub publisher_pinned: bool,
    pub last_result: Option<String>,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateRequest {
    pub version: String,
    pub drafts_clear: bool,
}
#[derive(Deserialize)]
struct Probe {
    version: Option<String>,
    notes: Option<String>,
    release_url: Option<String>,
    size: Option<u64>,
    #[serde(default)]
    unpublished: bool,
    #[serde(default)]
    current: bool,
}
pub struct Updater {
    dir: PathBuf,
    exe: PathBuf,
    port: u16,
    status: Mutex<UpdateStatus>,
    busy: AtomicBool,
}
struct Busy<'a>(&'a AtomicBool);
impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}
impl Updater {
    pub fn new(dir: PathBuf, port: u16) -> Result<Self> {
        if let Some(pin) = option_env!("TRACKSMITH_UPDATE_CERT_SHA256") {
            ensure!(
                pin.len() == 64 && pin.bytes().all(|b| b.is_ascii_hexdigit()),
                "Invalid configured update certificate pin"
            );
        }
        let exe = std::env::current_exe()?;
        let kind = if exe.with_file_name(".tracksmith-installed").is_file() {
            "installed"
        } else {
            "portable"
        };
        let last_result = (|| -> Result<String> {
            let path = dir.join("update-result.json");
            ensure!(
                fs::metadata(&path)?.len() <= 8192,
                "Update result too large"
            );
            let text = fs::read_to_string(path)?;
            let value: serde_json::Value =
                serde_json::from_str(text.trim_start_matches('\u{feff}'))?;
            Ok(value["message"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(4096)
                .collect())
        })()
        .ok();
        Ok(Self {
            dir,
            exe,
            port,
            busy: AtomicBool::new(false),
            status: Mutex::new(UpdateStatus {
                current: env!("CARGO_PKG_VERSION").into(),
                repository: REPOSITORY.into(),
                phase: "not_checked".into(),
                kind: kind.into(),
                available: None,
                notes: String::new(),
                release_url: None,
                size: None,
                checked_at: None,
                message: "Check GitHub Releases for updates.".into(),
                publisher_pinned: certificate().is_some(),
                last_result,
            }),
        })
    }
    pub fn status(&self) -> UpdateStatus {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    fn change(&self, f: impl FnOnce(&mut UpdateStatus)) {
        f(&mut self.status.lock().unwrap_or_else(|e| e.into_inner()));
    }
    fn begin(&self) -> Result<Busy<'_>> {
        ensure!(
            self.busy
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok(),
            "An update request is already running"
        );
        Ok(Busy(&self.busy))
    }
    fn stage(&self) -> Result<PathBuf> {
        let path = self
            .dir
            .join("updates")
            .join(uuid::Uuid::new_v4().simple().to_string());
        fs::create_dir_all(&path)?;
        fs::write(path.join("update.ps1"), SCRIPT)?;
        Ok(path)
    }
    fn command(&self, stage: &Path, mode: &str) -> Result<Command> {
        let system =
            std::env::var_os("SystemRoot").context("Windows system directory is unavailable")?;
        let mut command = Command::new(
            PathBuf::from(system).join("System32/WindowsPowerShell/v1.0/powershell.exe"),
        );
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        command
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(stage.join("update.ps1"))
            .args([
                "-Mode",
                mode,
                "-Repository",
                REPOSITORY,
                "-Current",
                env!("CARGO_PKG_VERSION"),
                "-Application",
            ])
            .arg(&self.exe)
            .arg("-DataDirectory")
            .arg(&self.dir)
            .args(["-Port", &self.port.to_string()]);
        if let Some(pin) = certificate() {
            command.args(["-Certificate", pin]);
        }
        Ok(command)
    }
    fn run(&self, command: &mut Command) -> Result<String> {
        let output = command
            .output()
            .context("Starting the Windows update worker")?;
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
                .trim()
                .chars()
                .take(2048)
                .collect::<String>()
        );
        ensure!(
            output.stdout.len() <= 64 * 1024,
            "Update response is too large"
        );
        Ok(String::from_utf8(output.stdout)?
            .trim()
            .trim_start_matches('\u{feff}')
            .into())
    }
    pub fn check(&self) -> Result<UpdateStatus> {
        let _busy = self.begin()?;
        self.change(|s| {
            s.phase = "checking".into();
            s.message = "Checking GitHub Releases…".into();
        });
        let result = (|| -> Result<()> {
            let stage = self.stage()?;
            let response = self
                .command(&stage, "Check")
                .and_then(|mut c| self.run(&mut c));
            let _ = fs::remove_dir_all(&stage);
            let probe: Probe = serde_json::from_str(&response?)?;
            if let Some(version) = &probe.version {
                ensure!(
                    version_parts(version).is_some()
                        && version_parts(version) > version_parts(env!("CARGO_PKG_VERSION")),
                    "Invalid newer release version"
                );
            }
            ensure!(
                probe.version.is_some() || probe.unpublished || probe.current,
                "Unrecognised update response"
            );
            self.change(|s| {
                s.available = probe.version;
                s.notes = probe.notes.unwrap_or_default();
                s.release_url = probe.release_url;
                s.size = probe.size;
                s.checked_at = Some(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                );
                s.phase = if probe.unpublished {
                    "unpublished"
                } else if s.available.is_some() {
                    "available"
                } else {
                    "current"
                }
                .into();
                s.message = if probe.unpublished {
                    "No public release has been published yet."
                } else if s.available.is_some() {
                    "An update is available."
                } else {
                    "You are up to date."
                }
                .into();
            });
            Ok(())
        })();
        if let Err(error) = result {
            self.failed(&error);
            return Err(error);
        }
        Ok(self.status())
    }
    pub fn install(
        &self,
        version: &str,
        desktop: &crate::lifecycle::Desktop,
    ) -> Result<UpdateStatus> {
        let _busy = self.begin()?;
        ensure!(
            version_parts(version).is_some() && self.status().available.as_deref() == Some(version),
            "Check for updates and choose the available release first"
        );
        self.change(|s| {
            s.phase = "preparing".into();
            s.message = "Downloading and verifying the update…".into();
        });
        let result = (|| -> Result<()> {
            let stage = self.stage()?;
            let prepared = self.command(&stage, "Prepare").and_then(|mut c| {
                self.run(c.args([
                    "-Target",
                    version,
                    "-WaitFor",
                    &std::process::id().to_string(),
                ]))
            });
            if let Err(error) = prepared {
                let _ = fs::remove_dir_all(&stage);
                return Err(error);
            }
            ensure!(
                !desktop.closing.load(Ordering::SeqCst),
                "Tracksmith is shutting down; update was not started"
            );
            self.command(&stage, "Apply")?
                .args([
                    "-Target",
                    version,
                    "-WaitFor",
                    &std::process::id().to_string(),
                ])
                .spawn()
                .context("Starting the update/restart worker")?;
            self.change(|s| {
                s.phase = "restarting".into();
                s.message = "Update verified. Tracksmith is restarting…".into();
            });
            desktop.stop();
            Ok(())
        })();
        if let Err(error) = result {
            self.failed(&error);
            return Err(error);
        }
        Ok(self.status())
    }
    fn failed(&self, error: &anyhow::Error) {
        self.change(|s| {
            s.phase = "error".into();
            s.message = format!("{error:#}");
        });
    }
}
fn certificate() -> Option<&'static str> {
    option_env!("TRACKSMITH_UPDATE_CERT_SHA256")
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
}
fn version_parts(value: &str) -> Option<[u16; 3]> {
    let parts: Vec<_> = value.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let mut result = [0; 3];
    for (index, part) in parts.into_iter().enumerate() {
        let number = part.parse::<u16>().ok()?;
        if part != number.to_string() {
            return None;
        }
        result[index] = number;
    }
    Some(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_versions_compare_numerically() {
        assert!(version_parts("1.10.0") > version_parts("1.9.9"));
        for invalid in ["v1.0.0", "1.0", "01.0.0", "1.0.0-rc.1", "65536.0.0"] {
            assert!(version_parts(invalid).is_none());
        }
    }
}
