//! Bounded, reviewed jobs. Only Engine::save performs writes.
use crate::media::{BatchTicket, Engine, model::Change};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use utoipa::ToSchema;

#[derive(Clone, Serialize, Deserialize, ToSchema)]
pub struct BatchRow {
    pub id: String,
    pub path: String,
    pub status: String,
    pub changes: Vec<Change>,
    pub warnings: Vec<String>,
    pub operation_id: Option<String>,
}
#[derive(Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct BatchView {
    pub id: String,
    pub phase: String,
    pub profile: String,
    pub revision: u32,
    pub current: String,
    pub rows: Vec<BatchRow>,
    pub messages: Vec<String>,
    pub elapsed_ms: u64,
    pub started_at: u64,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchRequest {
    pub sources: Vec<String>,
    pub recursive: bool,
    pub filter: String,
    pub profile_id: String,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchSelection {
    pub job_id: String,
    pub selected: Vec<String>,
}
#[derive(Default)]
struct Job {
    view: BatchView,
    tickets: HashMap<String, BatchTicket>,
}
#[derive(Default)]
pub struct Batch {
    job: Mutex<Job>,
    pub busy: AtomicBool,
    pub cancel: AtomicBool,
}
impl Batch {
    pub fn discard_review(&self) {
        if self.busy.load(Ordering::SeqCst) {
            return;
        }
        self.update(|j| {
            if j.view.phase == "review" {
                j.tickets.clear();
                j.view.phase = "cancelled".into();
                for row in &mut j.view.rows {
                    if row.status == "ready" {
                        row.status = "skipped".into();
                    }
                }
            }
        });
    }
    pub fn view(&self) -> BatchView {
        self.job
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .view
            .clone()
    }
    fn update(&self, f: impl FnOnce(&mut Job)) {
        f(&mut self.job.lock().unwrap_or_else(|e| e.into_inner()));
    }
    pub fn preview(
        self: &Arc<Self>,
        engine: Arc<Mutex<Engine>>,
        request: BatchRequest,
    ) -> Result<BatchView> {
        ensure!(
            self.busy
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok(),
            "Batch already running"
        );
        self.cancel.store(false, Ordering::SeqCst);
        self.update(|j| {
            *j = Job {
                view: BatchView {
                    id: uuid::Uuid::new_v4().to_string(),
                    phase: "planning".into(),
                    started_at: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    ..Default::default()
                },
                tickets: HashMap::new(),
            }
        });
        let batch = self.clone();
        tokio::task::spawn_blocking(move || {
            let started = Instant::now();
            let result = (|| -> Result<()> {
                let mut engine = engine
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Media engine unavailable"))?;
                let library = engine.rule_library()?;
                let profile = library
                    .profiles
                    .iter()
                    .find(|p| p.id == request.profile_id)
                    .context("Unknown profile")?;
                batch.update(|j| {
                    j.view.profile = profile.name.clone();
                    j.view.revision = library.revision;
                });
                let paths = engine.batch_paths(
                    &request.sources,
                    request.recursive,
                    &request.filter,
                    &batch.cancel,
                )?;
                let mut retained = 0;
                for path in paths {
                    if batch.cancel.load(Ordering::SeqCst) {
                        break;
                    }
                    batch.update(|j| j.view.current = path.display().to_string());
                    let id = uuid::Uuid::new_v4().to_string();
                    let mut row = BatchRow {
                        id: id.clone(),
                        path: path.display().to_string(),
                        status: "error".into(),
                        changes: vec![],
                        warnings: vec![],
                        operation_id: None,
                    };
                    match engine.prepare_batch(&path, &library, &request.profile_id) {
                        Ok(ticket) => {
                            row.status = if ticket.changes.is_empty() {
                                "unchanged"
                            } else {
                                "ready"
                            }
                            .into();
                            row.changes = ticket.changes.clone();
                            row.warnings = ticket.warnings.clone();
                            if !ticket.changes.is_empty() {
                                batch.update(|j| {
                                    j.tickets.insert(id, ticket);
                                });
                            }
                        }
                        Err(error) => row.warnings.push(format!("{error:#}")),
                    }
                    bound_warnings(&mut row.warnings);
                    retained += serde_json::to_vec(&row)?.len();
                    ensure!(
                        retained <= 8 * 1024 * 1024,
                        "Batch review exceeds 8 MiB; choose fewer files"
                    );
                    batch.update(|j| {
                        j.view.rows.push(row);
                        j.view.elapsed_ms = started.elapsed().as_millis() as u64;
                    });
                }
                Ok(())
            })();
            batch.update(|j| {
                j.view.phase = if batch.cancel.load(Ordering::SeqCst) {
                    "cancelled"
                } else if result.is_ok() {
                    "review"
                } else {
                    "failed"
                }
                .into();
                if let Err(error) = result {
                    j.view.messages.push(format!("{error:#}"));
                    j.tickets.clear();
                }
                if j.view.phase == "cancelled" {
                    j.tickets.clear();
                }
                j.view.current.clear();
                j.view.elapsed_ms = started.elapsed().as_millis() as u64;
            });
            batch.busy.store(false, Ordering::SeqCst);
        });
        Ok(self.view())
    }
    pub fn execute(
        self: &Arc<Self>,
        engine: Arc<Mutex<Engine>>,
        input: BatchSelection,
        dir: String,
    ) -> Result<BatchView> {
        ensure!(!input.selected.is_empty(), "Select files to save");
        let selected: HashSet<String> = input.selected.into_iter().collect();
        let mut job = self.job.lock().unwrap_or_else(|e| e.into_inner());
        ensure!(
            job.view.id == input.job_id && job.view.phase == "review",
            "Preview this batch again"
        );
        ensure!(
            selected.iter().all(|id| job.tickets.contains_key(id)),
            "Selection contains an unavailable file"
        );
        ensure!(
            self.busy
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok(),
            "Batch already running"
        );
        self.cancel.store(false, Ordering::SeqCst);
        job.view.phase = "saving".into();
        let tickets: Vec<_> = job
            .view
            .rows
            .iter()
            .filter(|r| selected.contains(&r.id))
            .map(|r| r.id.clone())
            .collect();
        drop(job);
        let batch = self.clone();
        tokio::task::spawn_blocking(move || {
            let started = Instant::now();
            for id in tickets {
                if batch.cancel.load(Ordering::SeqCst) {
                    break;
                }
                let ticket = {
                    let mut job = batch.job.lock().unwrap_or_else(|e| e.into_inner());
                    job.view.current = job
                        .view
                        .rows
                        .iter()
                        .find(|r| r.id == id)
                        .map(|r| r.path.clone())
                        .unwrap_or_default();
                    job.tickets.remove(&id)
                };
                let result = engine
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Media engine unavailable"))
                    .and_then(|mut e| e.commit_batch(ticket.expect("validated ticket")));
                let failed = !result.as_ref().is_ok_and(|r| r.success);
                batch.update(|j| {
                    let row = j.view.rows.iter_mut().find(|r| r.id == id).unwrap();
                    match result {
                        Ok(report) => {
                            row.status = report.status;
                            row.operation_id = Some(report.operation_id);
                            let mut warnings = report.discrepancies;
                            warnings.extend(report.warnings);
                            warnings.append(&mut row.warnings);
                            row.warnings = warnings;
                        }
                        Err(error) => {
                            row.status = "error".into();
                            row.warnings.insert(0, format!("{error:#}"));
                        }
                    }
                    bound_warnings(&mut row.warnings);
                    j.view.elapsed_ms = started.elapsed().as_millis() as u64;
                });
                if failed {
                    batch.update(|j|j.view.messages.push("Stopped after a failed save or verification. Review the affected file before continuing.".into()));
                    break;
                }
            }
            batch.update(|j| {
                for row in &mut j.view.rows {
                    if row.status == "ready" {
                        row.status = "skipped".into();
                    }
                }
                j.tickets.clear();
                j.view.phase = if batch.cancel.load(Ordering::SeqCst) {
                    "cancelled"
                } else {
                    "finished"
                }
                .into();
                j.view.current.clear();
            });
            let view = batch.view();
            if let Err(error) = persist(Path::new(&dir), &view) {
                batch.update(|j| {
                    j.view
                        .messages
                        .push(format!("Batch report could not be saved: {error:#}"))
                });
            }
            batch.busy.store(false, Ordering::SeqCst);
        });
        Ok(self.view())
    }
}
fn bound_warnings(warnings: &mut Vec<String>) {
    if warnings.iter().map(String::len).sum::<usize>() + warnings.len() <= 8192 {
        return;
    }
    let text = warnings.join("\n");
    let mut end = 8192.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    *warnings = vec![format!(
        "{}\n[Warning summary truncated to 8 KiB]",
        &text[..end]
    )];
}
fn persist(dir: &Path, view: &BatchView) -> Result<()> {
    let folder = dir.join("batches");
    std::fs::create_dir_all(&folder)?;
    let mut file = tempfile::NamedTempFile::new_in(&folder)?;
    serde_json::to_writer(&mut file, view)?;
    file.as_file().sync_all()?;
    file.persist(folder.join(format!("{}.json", view.id)))?;
    Ok(())
}
pub fn history(dir: &Path) -> Result<Vec<BatchView>> {
    let folder = dir.join("batches");
    if !folder.exists() {
        return Ok(vec![]);
    }
    let mut paths: Vec<_> = std::fs::read_dir(folder)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort_by_key(|e| std::cmp::Reverse(e.metadata().and_then(|m| m.modified()).ok()));
    let mut retained = 0;
    let mut result = vec![];
    for entry in paths.into_iter().take(20) {
        let size = entry.metadata()?.len();
        ensure!(size <= 16 * 1024 * 1024, "Report too large");
        if retained + size > 32 * 1024 * 1024 {
            break;
        }
        retained += size;
        result.push(serde_json::from_slice(&std::fs::read(entry.path())?)?);
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn batches_accept_more_than_500_files() {
        let dir = tempfile::tempdir().unwrap();
        for n in 0..501 {
            std::fs::write(dir.path().join(format!("{n}.mkv")), b"").unwrap();
        }
        let config = crate::config::Config {
            media_roots: vec![dir.path().to_path_buf()],
            ..Default::default()
        };
        let engine = Engine::new(&config, dir.path().join("config")).unwrap();
        let paths = engine
            .batch_paths(
                &[dir.path().display().to_string()],
                false,
                "",
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(paths.len(), 501);
        let batch = Arc::new(Batch::default());
        let result = batch.execute(
            Arc::new(Mutex::new(engine)),
            BatchSelection {
                job_id: "unreviewed".into(),
                selected: (0..501).map(|n| n.to_string()).collect(),
            },
            dir.path().display().to_string(),
        );
        assert!(
            result
                .err()
                .unwrap()
                .to_string()
                .contains("Preview this batch again")
        );
    }
}
