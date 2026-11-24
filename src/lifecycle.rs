//! Instance lock and authenticated local launch handoff. Browser requests cannot grant paths.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use utoipa::ToSchema;

#[derive(Serialize, Deserialize)]
pub struct Session {
    pub port: u16,
    pub token: String,
    pub launch_key: String,
}
pub struct Instance {
    _lock: File,
    dir: PathBuf,
}
impl Instance {
    pub fn acquire(dir: &Path) -> Result<Option<Self>> {
        std::fs::create_dir_all(dir)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.join("instance.lock"))?;
        match lock.try_lock() {
            Ok(()) => Ok(Some(Self {
                _lock: lock,
                dir: dir.into(),
            })),
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
    pub fn publish(&self, session: &Session) -> Result<()> {
        let mut file = tempfile::NamedTempFile::new_in(&self.dir)?;
        serde_json::to_writer(&mut file, session)?;
        file.as_file().sync_all()?;
        file.persist(self.dir.join("session.json"))?;
        Ok(())
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.dir.join("session.json"));
    }
}
#[derive(Clone, Serialize, ToSchema)]
pub struct LaunchItem {
    pub id: String,
    pub path: String,
    pub folder: bool,
}
pub struct Desktop {
    pub launch_key: String,
    pub url: String,
    pub closing: AtomicBool,
    pub updating: AtomicBool,
    pub shutdown: tokio::sync::Notify,
    pending: Mutex<Vec<LaunchItem>>,
}
impl Desktop {
    pub fn new(url: String, launch_key: String) -> Self {
        Self {
            launch_key,
            url,
            closing: AtomicBool::new(false),
            updating: AtomicBool::new(false),
            shutdown: tokio::sync::Notify::new(),
            pending: Mutex::new(vec![]),
        }
    }
    pub fn enqueue(&self, path: &Path) -> Result<()> {
        ensure!(
            !self.closing.load(Ordering::SeqCst) && !self.updating.load(Ordering::SeqCst),
            "Tracksmith is shutting down"
        );
        // Use the same native-selection path validation without retaining a grant until explicit UI action.
        let canonical = crate::media::validate_selection(path)?;
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        ensure!(pending.len() < 32, "Too many pending Explorer selections");
        pending.push(LaunchItem {
            id: uuid::Uuid::new_v4().to_string(),
            folder: canonical.is_dir(),
            path: canonical.display().to_string(),
        });
        Ok(())
    }
    pub fn pending(&self) -> Vec<LaunchItem> {
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    pub fn selected(&self, id: &str) -> Result<LaunchItem> {
        self.pending()
            .into_iter()
            .find(|p| p.id == id)
            .context("Launch selection expired")
    }
    pub fn dismiss(&self, id: &str) {
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|p| p.id != id);
    }
    pub fn stop(&self) {
        self.closing.store(true, Ordering::SeqCst);
        self.shutdown.notify_one();
    }
}
pub fn forward(dir: &Path, path: Option<&Path>, background: bool) -> Result<()> {
    let mut last = None;
    for _ in 0..20 {
        match forward_once(dir, path) {
            Ok(session) => {
                if !background {
                    crate::desktop::open_url(&format!(
                        "http://127.0.0.1:{}/#token={}",
                        session.port, session.token
                    ))?;
                }
                return Ok(());
            }
            Err(error) => last = Some(error),
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    Err(last.unwrap()).context("Existing Tracksmith instance could not accept this launch")
}
fn forward_once(dir: &Path, path: Option<&Path>) -> Result<Session> {
    let session_path = dir.join("session.json");
    ensure!(
        std::fs::metadata(&session_path)?.len() <= 4096,
        "Invalid session file"
    );
    let session: Session = serde_json::from_slice(&std::fs::read(session_path)?)?;
    ensure!(session.port != 0, "Invalid session port");
    for value in [&session.token, &session.launch_key] {
        uuid::Uuid::parse_str(value)?;
    }
    let body =
        serde_json::to_string(&serde_json::json!({"path":path.map(|p|p.display().to_string())}))?;
    let mut stream = TcpStream::connect_timeout(
        &SocketAddr::from((Ipv4Addr::LOCALHOST, session.port)),
        Duration::from_secs(2),
    )?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    write!(
        stream,
        "POST /api/desktop/launch HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\nX-Tracksmith-Launch: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        session.port,
        session.token,
        session.launch_key,
        body.len(),
        body
    )?;
    let mut response = String::new();
    stream.take(8192).read_to_string(&mut response)?;
    ensure!(
        response.starts_with("HTTP/1.1 200 "),
        "Existing instance refused launch"
    );
    Ok(session)
}
