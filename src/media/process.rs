use anyhow::{Context, Result, ensure};
use std::{
    ffi::OsString,
    io::Read,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const OUTPUT_LIMIT: usize = 16 * 1024 * 1024;
pub struct Output {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: String,
    pub truncated: bool,
}
impl Output {
    pub fn checked(self) -> Result<Vec<u8>> {
        ensure!(
            !self.truncated,
            "Utility output exceeded the 16 MiB inspection limit"
        );
        ensure!(
            self.code == Some(0),
            "Utility failed ({:?}): {} {}",
            self.code,
            self.stderr,
            String::from_utf8_lossy(&self.stdout)
        );
        Ok(self.stdout)
    }
}
fn drain(mut stream: impl Read) -> (Vec<u8>, bool) {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    let mut truncated = false;
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                let remaining = OUTPUT_LIMIT.saturating_sub(bytes.len());
                bytes.extend_from_slice(&buffer[..count.min(remaining)]);
                truncated |= count > remaining;
            }
            Err(_) => {
                truncated = true;
                break;
            }
        }
    }
    (bytes, truncated)
}

/// Inspections may time out. An active metadata write is always allowed to finish.
pub fn run(exe: &Path, args: &[OsString], writing: bool) -> Result<Output> {
    let mut command = Command::new(exe);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
        .env_remove("MKVTOOLNIX_OPTIONS")
        .env_remove("MKVPROPEDIT_OPTIONS");
    for name in [
        "MKVTOOLNIX_DEBUG",
        "MTX_DEBUG",
        "MKVPROPEDIT_DEBUG",
        "MKVMERGE_DEBUG",
        "MKVEXTRACT_DEBUG",
    ] {
        command.env_remove(name);
    }
    if let Some(folder) = exe.parent() {
        command.current_dir(folder);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("Starting {}", exe.display()))?;
    let stdout = child.stdout.take().context("Missing utility stdout")?;
    let stderr = child.stderr.take().context("Missing utility stderr")?;
    let out_reader = thread::spawn(move || drain(stdout));
    let err_reader = thread::spawn(move || drain(stderr));
    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if !writing && started.elapsed() > Duration::from_secs(90) {
            timed_out = true;
            child.kill()?;
            break child.wait()?;
        }
        thread::sleep(Duration::from_millis(20));
    };
    let (stdout, out_truncated) = out_reader
        .join()
        .map_err(|_| anyhow::anyhow!("Utility output reader failed"))?;
    let (stderr, err_truncated) = err_reader
        .join()
        .map_err(|_| anyhow::anyhow!("Utility error reader failed"))?;
    ensure!(
        !timed_out,
        "Inspection exceeded 90 seconds; no metadata write was started"
    );
    Ok(Output {
        code: status.code(),
        stdout,
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        truncated: out_truncated || err_truncated,
    })
}
