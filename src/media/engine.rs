use super::{
    adapter::ContainerAdapter,
    filesystem::{Boundary, Identity, hold_file_identity, preflight_write},
    matroska::Matroska,
    model::*,
};
use crate::config::Config;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

struct Snapshot {
    document: Inspection,
    identity: Identity,
    created: Instant,
    analysis_running: bool,
    cancel: Arc<AtomicBool>,
}
impl Drop for Snapshot {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

/// An authorised read operation that owns everything needed outside the engine lock.
pub struct InspectionTask {
    adapter: Arc<dyn ContainerAdapter>,
    path: PathBuf,
    identity: Identity,
    document: Option<Inspection>,
    retry: bool,
    cancel: Arc<AtomicBool>,
    batch: Option<bool>,
    full_analysis: bool,
    technical: bool,
}
impl InspectionTask {
    /// Explicit second opinion, even when the profile needs no technical facts.
    pub(crate) fn require_full_analysis(&mut self) {
        self.batch = Some(true);
        self.full_analysis = true;
        self.retry = true;
    }

    pub(crate) fn refresh_technical(&self) -> Result<()> {
        self.identity.verify(&self.path)?;
        self.adapter.invalidate_technical(&self.path)
    }
    /// Read tracks once, ahead of the bounded batch enrichment worker.
    pub(crate) fn identify_batch(
        &mut self,
        library: &super::rules::model::RuleLibrary,
        profile: &str,
    ) -> Result<()> {
        self.identity.verify(&self.path)?;
        let document = self.adapter.inspect_quick(&self.path)?;
        self.identity.verify(&self.path)?;
        ensure!(
            serde_json::to_vec(&document)?.len() <= 8 * 1024 * 1024,
            "Inspection metadata exceeds the 8 MiB session limit"
        );
        let (technical, full) = super::rules::requirements_for(library, profile, &document)?;
        self.batch = Some(technical);
        self.full_analysis = full;
        self.document = Some(document);
        Ok(())
    }

    pub(crate) fn cancellation_token(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }
    pub fn run(&self) -> Result<Inspection> {
        ensure!(
            !self.cancel.load(Ordering::SeqCst),
            "Inspection was closed or replaced"
        );
        self.identity.verify(&self.path)?;
        let document = if let Some(document) = &self.document {
            let mut document = document.clone();
            let technical = if let Some(enrich) = self.batch {
                enrich
                    && (self.retry
                        || document.analysis.status == "pending"
                        || (self.full_analysis && document.analysis.scope != "full"))
            } else {
                self.adapter.complete_sections(&mut document, &self.cancel);
                self.technical
            };
            if technical {
                self.adapter.complete_analysis(
                    document,
                    self.retry || self.full_analysis,
                    &self.cancel,
                )?
            } else {
                document
            }
        } else if let Some(enrich) = self.batch {
            self.adapter
                .inspect_batch(&self.path, enrich, self.full_analysis, &self.cancel)?
        } else {
            self.adapter.inspect_quick(&self.path)?
        };
        self.identity.verify(&self.path)?;
        ensure!(
            serde_json::to_vec(&document)?.len() <= 8 * 1024 * 1024,
            "Inspection metadata exceeds the 8 MiB session limit"
        );
        Ok(document)
    }
}
struct Staged {
    plan: Plan,
    created: Instant,
}
pub struct BatchTicket {
    identity: Identity,
    path: PathBuf,
    edits: Vec<Edit>,
    pub changes: Vec<Change>,
    pub warnings: Vec<String>,
    document: Inspection,
}
impl BatchTicket {
    pub fn row_status(&self) -> &'static str {
        if !self.changes.is_empty() {
            "ready"
        } else if !self.warnings.is_empty() {
            "needs_review"
        } else {
            "unchanged"
        }
    }
    pub fn retained_bytes(&self) -> Result<usize> {
        Ok(serde_json::to_vec(&self.document)?.len())
    }
}
#[derive(Serialize, Deserialize)]
struct Record {
    id: String,
    path: String,
    changes: Vec<Change>,
    status: String,
    report: Option<SaveReport>,
}

pub struct Engine {
    adapter: Arc<dyn ContainerAdapter>,
    boundary: Boundary,
    config_dir: PathBuf,
    snapshots: HashMap<String, Snapshot>,
    plans: HashMap<String, Staged>,
}
impl Engine {
    fn full_analysis_required(&self) -> bool {
        self.rule_library()
            .and_then(|library| {
                super::rules::needs_full_media_info(&library, &library.active_profile_id)
            })
            .unwrap_or(true)
    }
    pub fn inspection_task(&self, input: &Path) -> Result<InspectionTask> {
        let path = self.boundary.authorize(input)?;
        let technical = self
            .rule_library()
            .and_then(|library| {
                super::rules::needs_media_info(&library, &library.active_profile_id)
            })
            .unwrap_or(true);
        Ok(InspectionTask {
            identity: Identity::capture(&path)?,
            path,
            adapter: self.adapter.clone(),
            document: None,
            retry: false,
            cancel: Arc::new(AtomicBool::new(false)),
            batch: None,
            full_analysis: self.full_analysis_required(),
            technical,
        })
    }

    pub fn finish_inspection(
        &mut self,
        task: InspectionTask,
        mut document: Inspection,
    ) -> Result<Inspection> {
        self.prune();
        task.identity
            .verify(&self.boundary.authorize(&task.path)?)?;
        let (technical, full) = self
            .rule_library()
            .and_then(|library| {
                super::rules::requirements_for(&library, &library.active_profile_id, &document)
            })
            .unwrap_or((true, true));
        if !technical && document.analysis.status == "pending" {
            document.analysis.status = "not_requested".into();
        }
        if full
            && document.analysis.scope != "full"
            && document
                .tracks
                .iter()
                .any(|t| matches!(t.kind.as_str(), "audio" | "video"))
        {
            document.analysis.status = "pending".into();
        }
        self.store_inspection(document, task.identity, task.cancel)
    }

    pub fn analysis_task(
        &mut self,
        id: &str,
        retry: bool,
    ) -> Result<(Inspection, Option<InspectionTask>)> {
        self.prune();
        let (technical_required, full_analysis) = self
            .rule_library()
            .and_then(|library| {
                let document = &self
                    .snapshots
                    .get(id)
                    .context("Inspection expired; reopen the file")?
                    .document;
                super::rules::requirements_for(&library, &library.active_profile_id, document)
            })
            .unwrap_or((true, true));
        let snapshot = self
            .snapshots
            .get_mut(id)
            .context("Inspection expired; reopen the file")?;
        let path = self
            .boundary
            .authorize(Path::new(&snapshot.document.path))?;
        if let Err(error) = snapshot.identity.verify(&path) {
            snapshot.cancel.store(true, Ordering::SeqCst);
            snapshot.analysis_running = false;
            let message = format!("File changed during analysis: {error:#}. Reopen the file.");
            snapshot.document.analysis = TechnicalState {
                status: "failed".into(),
                scope: "none".into(),
                cached: false,
                error: Some(message.clone()),
            };
            snapshot.document.sections_pending = false;
            if !snapshot.document.warnings.contains(&message) {
                snapshot.document.warnings.push(message);
            }
            return Ok((snapshot.document.clone(), None));
        }
        if !snapshot.analysis_running
            && ((full_analysis && snapshot.document.analysis.scope == "audio")
                || (technical_required && snapshot.document.analysis.status == "not_requested"))
        {
            snapshot.document.analysis.status = "pending".into();
        }
        if snapshot.analysis_running
            || (!retry
                && snapshot.document.analysis.status != "pending"
                && !snapshot.document.sections_pending)
        {
            return Ok((snapshot.document.clone(), None));
        }
        snapshot.analysis_running = true;
        let technical = retry || snapshot.document.analysis.status == "pending";
        if technical {
            snapshot.document.analysis.status = "pending".into();
        }
        let task = InspectionTask {
            adapter: self.adapter.clone(),
            identity: Identity::capture(&path)?,
            path,
            document: Some(snapshot.document.clone()),
            retry: retry || full_analysis,
            cancel: snapshot.cancel.clone(),
            batch: None,
            full_analysis,
            technical,
        };
        Ok((snapshot.document.clone(), Some(task)))
    }

    pub fn finish_analysis(&mut self, task: InspectionTask, result: Result<Inspection>) {
        let Some(id) = task.document.as_ref().map(|d| &d.id) else {
            return;
        };
        let Some(snapshot) = self.snapshots.get_mut(id) else {
            return;
        };
        if !Arc::ptr_eq(&snapshot.cancel, &task.cancel) {
            return;
        }
        snapshot.analysis_running = false;
        let result = result.and_then(|document| {
            task.identity
                .verify(&self.boundary.authorize(&task.path)?)?;
            snapshot.identity.verify(&task.path)?;
            Ok(document)
        });
        match result {
            Ok(mut document) => {
                if snapshot.document.chapters.available && !document.chapters.available {
                    document.chapters = snapshot.document.chapters.clone();
                }
                if snapshot.document.tags.available && !document.tags.available {
                    document.tags = snapshot.document.tags.clone();
                }
                snapshot.document = document;
            }
            Err(error) => {
                let message = format!(
                    "Technical analysis unavailable: {error:#}. Reopen the file before retrying."
                );
                snapshot.document.analysis = TechnicalState {
                    status: "failed".into(),
                    scope: "none".into(),
                    cached: false,
                    error: Some(message.clone()),
                };
                snapshot.document.sections_pending = false;
                snapshot.document.warnings.push(message);
            }
        }
    }

    pub fn cancel_analysis(&mut self, id: &str) -> Result<Inspection> {
        let snapshot = self.snapshots.get_mut(id).context("Inspection expired")?;
        if snapshot.analysis_running
            || snapshot.document.analysis.status == "pending"
            || snapshot.document.sections_pending
        {
            snapshot.cancel.store(true, Ordering::SeqCst);
            snapshot.cancel = Arc::new(AtomicBool::new(false));
            snapshot.analysis_running = false;
            snapshot.document.sections_pending = false;
            snapshot.document.analysis.status = if snapshot.document.mediainfo.is_some() {
                "partial"
            } else {
                "failed"
            }
            .into();
            snapshot.document.analysis.error =
                Some("Technical analysis stopped; retry when needed.".into());
            for section in [&mut snapshot.document.chapters, &mut snapshot.document.tags] {
                if !section.available {
                    section.error = Some("Loading stopped; retry to inspect".into());
                }
            }
        }
        Ok(snapshot.document.clone())
    }

    pub fn cancel_all_analysis(&mut self) {
        for snapshot in self.snapshots.values() {
            snapshot.cancel.store(true, Ordering::SeqCst);
        }
    }

    pub fn fail_analysis(&mut self, id: &str, generation: &Arc<AtomicBool>, message: String) {
        if let Some(snapshot) = self.snapshots.get_mut(id)
            && Arc::ptr_eq(generation, &snapshot.cancel)
        {
            snapshot.analysis_running = false;
            snapshot.document.analysis.status = "failed".into();
            snapshot.document.sections_pending = false;
            snapshot.document.analysis.error = Some(message);
        }
    }

    pub fn selected_inspection_task(&mut self, path: &Path) -> Result<InspectionTask> {
        let canonical = self.boundary.grant_selected_file(path)?;
        self.inspection_task(&canonical)
    }

    pub(crate) fn grant_batch_file(&mut self, path: &Path) -> Result<String> {
        Ok(self
            .boundary
            .grant_selected_file(path)?
            .display()
            .to_string())
    }
    pub fn batch_paths(
        &self,
        sources: &[String],
        recursive: bool,
        filter: &str,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<Vec<PathBuf>> {
        ensure!(
            !sources.is_empty() && sources.len() <= 32,
            "Choose between 1 and 32 sources"
        );
        ensure!(filter.len() <= 256, "Filter is too long");
        let mut pending = vec![];
        let mut files = std::collections::BTreeSet::new();
        for source in sources {
            let path = Path::new(source);
            if path.is_dir() {
                pending.push((self.boundary.authorize_directory(path)?, 0));
            } else {
                files.insert(self.boundary.authorize(path)?);
            }
        }
        let mut visited = 0;
        let mut entries = 0;
        while let Some((folder, depth)) = pending.pop() {
            ensure!(
                !cancel.load(std::sync::atomic::Ordering::SeqCst),
                "Discovery cancelled"
            );
            visited += 1;
            ensure!(
                visited <= 10000 && depth <= 32,
                "Folder discovery limit exceeded; choose a smaller source"
            );
            for entry in fs::read_dir(folder)? {
                entries += 1;
                ensure!(
                    entries <= 100000,
                    "Discovery exceeds 100,000 entries; choose a smaller source"
                );
                ensure!(
                    !cancel.load(std::sync::atomic::Ordering::SeqCst),
                    "Discovery cancelled"
                );
                let entry = entry?;
                let path = entry.path();
                let metadata = fs::symlink_metadata(&path)?;
                if metadata.file_type().is_symlink() {
                    continue;
                }
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if metadata.file_attributes() & 0x400 != 0 {
                        continue;
                    }
                }
                if metadata.is_dir() && recursive {
                    pending.push((self.boundary.authorize_directory(&path)?, depth + 1));
                } else if metadata.is_file()
                    && path.extension().and_then(|v| v.to_str()).is_some_and(|v| {
                        v.eq_ignore_ascii_case("mkv") || v.eq_ignore_ascii_case("mka")
                    })
                    && entry
                        .file_name()
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&filter.to_lowercase())
                {
                    files.insert(self.boundary.authorize(&path)?);
                }
                ensure!(
                    pending.len() <= 10000,
                    "Pending folder limit exceeded; narrow your sources"
                );
            }
        }
        Ok(files
            .into_iter()
            .filter(|p| {
                p.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase()
                    .contains(&filter.to_lowercase())
            })
            .collect())
    }
    pub fn batch_inspection_task(
        &self,
        path: &Path,
        enrich: bool,
        full: bool,
    ) -> Result<InspectionTask> {
        let path = self.boundary.authorize(path)?;
        Ok(InspectionTask {
            identity: Identity::capture(&path)?,
            path,
            adapter: self.adapter.clone(),
            document: None,
            retry: false,
            cancel: Arc::new(AtomicBool::new(false)),
            batch: Some(enrich),
            full_analysis: full,
            technical: false,
        })
    }

    pub fn review_batch(
        &mut self,
        task: InspectionTask,
        document: Inspection,
        library: &super::rules::model::RuleLibrary,
        profile_id: &str,
    ) -> Result<BatchTicket> {
        let path = self.boundary.authorize(&task.path)?;
        task.identity.verify(&path)?;
        let identity = task.identity;
        let started = Instant::now();
        let inspection_ms = document.elapsed_ms;
        ensure!(
            document.write_available,
            "Writing is unavailable for this file"
        );
        let request = super::rules::model::RulePreviewRequest {
            inspection_id: document.id.clone(),
            profile_id: profile_id.into(),
            manual_edits: vec![],
            test_rule: None,
            test_library: None,
        };
        let preview = super::rules::evaluate(&document, library, &request)?;
        let edits: Vec<Edit> = preview.proposals.into_iter().map(|p| p.edit).collect();
        let mut changes = vec![];
        for edit in &edits {
            if let Some(change) = self.adapter.plan_change(&document, edit.clone())? {
                changes.push(change);
            }
        }
        let mut warnings = document.warnings.clone();
        warnings.extend(preview.notes);
        for conflict in preview.conflicts {
            warnings.push(format!(
                "Rule conflict resolved by priority: {}",
                serde_json::to_string(&conflict)?
            ));
        }
        warnings.extend(self.adapter.validate_plan(&document, &changes)?);
        identity.verify(&self.boundary.authorize(&path)?)?;
        tracing::debug!(
            inspection_ms,
            planning_ms = started.elapsed().as_millis(),
            "Batch file prepared"
        );
        Ok(BatchTicket {
            identity,
            path,
            edits,
            changes,
            warnings,
            document,
        })
    }
    pub fn commit_batch(&mut self, ticket: BatchTicket) -> Result<SaveReport> {
        ticket
            .identity
            .verify(&self.boundary.authorize(&ticket.path)?)?;
        let mut document = self.adapter.inspect_verification(&ticket.path)?;
        ticket
            .identity
            .verify(&self.boundary.authorize(&ticket.path)?)?;
        self.adapter
            .retain_technical(&ticket.document, &mut document);
        let document = self.store_inspection(
            document,
            Identity::capture(&ticket.path)?,
            Arc::new(AtomicBool::new(false)),
        )?;
        let plan = self.plan(&document.id, ticket.edits)?;
        ensure!(
            serde_json::to_vec(&plan.changes)? == serde_json::to_vec(&ticket.changes)?,
            "Metadata changed since batch review; preview again"
        );
        self.save(&plan.id)
    }
    pub fn new(config: &Config, config_dir: PathBuf) -> Result<Self> {
        let mut adapter = Matroska::new(&config.tools, &config_dir);
        let mut available_roots = vec![];
        for root in &config.media_roots {
            match Boundary::new(std::slice::from_ref(root)) {
                Ok(_) => available_roots.push(root.clone()),
                Err(error) => adapter.diagnostics.push(format!(
                    "Media location unavailable (excluded this session): {}: {error:#}",
                    root.display()
                )),
            }
        }
        Ok(Self {
            adapter: Arc::new(adapter),
            boundary: Boundary::new(&available_roots)?,
            config_dir,
            snapshots: HashMap::new(),
            plans: HashMap::new(),
        })
    }
    pub fn status(&self) -> EngineStatus {
        let status = self.adapter.status();
        EngineStatus {
            roots: self.boundary.roots(),
            inspection_available: status.inspection_available,
            writing_available: status.writing_available,
            diagnostics: status.diagnostics,
        }
    }
    pub fn set_locations(&mut self, roots: Vec<PathBuf>) -> Result<EngineStatus> {
        ensure!(
            roots.len() <= 64,
            "At most 64 media locations may be configured"
        );
        let boundary = Boundary::new(&roots)?;
        let mut config = Config::load(&self.config_dir)?;
        config.favourite_locations.retain(|folder| {
            roots
                .iter()
                .any(|root| fs::canonicalize(root).ok().as_ref() == Some(folder))
        });
        config.media_roots = roots;
        config.save(&self.config_dir)?;
        self.boundary = boundary;
        self.snapshots.clear();
        self.plans.clear();
        Ok(self.status())
    }
    fn prune(&mut self) {
        self.snapshots
            .retain(|_, s| s.created.elapsed() < Duration::from_secs(1800));
        self.plans
            .retain(|_, p| p.created.elapsed() < Duration::from_secs(600));
    }
    pub fn inspect(&mut self, input: &Path) -> Result<Inspection> {
        self.prune();
        let path = self.boundary.authorize(input)?;
        let identity = Identity::capture(&path)?;
        let document = self.adapter.inspect_quick(&path)?;
        identity.verify(&self.boundary.authorize(&path)?)?;
        self.store_inspection(document, identity, Arc::new(AtomicBool::new(false)))
    }
    fn store_inspection(
        &mut self,
        mut document: Inspection,
        identity: Identity,
        cancel: Arc<AtomicBool>,
    ) -> Result<Inspection> {
        let path = Path::new(&document.path);
        ensure!(
            serde_json::to_vec(&document)?.len() <= 8 * 1024 * 1024,
            "Inspection metadata exceeds the 8 MiB session limit"
        );
        identity.verify(&self.boundary.authorize(path)?)?;
        if let Err(error) = self.remember(path, false) {
            document
                .warnings
                .push(format!("Recent-file history could not be saved: {error:#}"));
        }
        if self.snapshots.len() >= 8
            && let Some(id) = self
                .snapshots
                .iter()
                .min_by_key(|(_, s)| s.created)
                .map(|(id, _)| id.clone())
        {
            self.snapshots.remove(&id);
        }
        self.snapshots.insert(
            document.id.clone(),
            Snapshot {
                document: document.clone(),
                identity,
                created: Instant::now(),
                analysis_running: false,
                cancel,
            },
        );
        Ok(document)
    }
    fn remember(&self, path: &Path, folder: bool) -> Result<()> {
        let mut config = Config::load(&self.config_dir)?;
        let history = if folder {
            &mut config.recent_locations
        } else {
            &mut config.recent_files
        };
        if history.first().is_some_and(|item| item == path) {
            return Ok(());
        }
        history.retain(|item| item != path);
        history.insert(0, path.to_path_buf());
        history.truncate(20);
        config.save(&self.config_dir)
    }
    pub fn browse_directory(
        &self,
        path: Option<&Path>,
        offset: usize,
        search: &str,
    ) -> Result<DirectoryListing> {
        let mut listing = super::browser::list(
            &self.boundary,
            &Config::load(&self.config_dir)?,
            path,
            offset,
            search,
        )?;
        if let Some(path) = &listing.path
            && let Err(error) = self.remember(Path::new(path), true)
        {
            listing.notes.push(format!(
                "Recent-location history could not be saved: {error:#}"
            ));
        }
        Ok(listing)
    }
    pub(crate) fn open_selected_folder(&mut self, path: &Path) -> Result<DirectoryListing> {
        let canonical = self.boundary.grant_selected_folder(path)?;
        self.remember(&canonical, true)?;
        self.browse_directory(Some(&canonical), 0, "")
    }
    pub fn recent_inspection_task(&mut self, path: &Path) -> Result<InspectionTask> {
        ensure!(
            Config::load(&self.config_dir)?
                .recent_files
                .iter()
                .any(|file| file == path),
            "File is not in Tracksmith's recent-file history; use Open file"
        );
        self.selected_inspection_task(path)
    }
    pub fn open_recent_location(&mut self, path: &Path) -> Result<DirectoryListing> {
        let config = Config::load(&self.config_dir)?;
        ensure!(
            config
                .recent_locations
                .iter()
                .chain(&config.favourite_locations)
                .any(|folder| folder == path),
            "Location is not in Tracksmith's history; use Choose folder"
        );
        self.open_selected_folder(path)
    }
    pub fn set_favourite(&mut self, path: &Path, favourite: bool) -> Result<EngineStatus> {
        let canonical = self.boundary.authorize_directory(path)?;
        let mut config = Config::load(&self.config_dir)?;
        config
            .favourite_locations
            .retain(|folder| folder != &canonical);
        if favourite {
            ensure!(
                config.favourite_locations.len() < 32,
                "At most 32 favourite locations are supported"
            );
            config.favourite_locations.push(canonical.clone());
            if !config.media_roots.contains(&canonical) {
                ensure!(
                    config.media_roots.len() < 64,
                    "Remove an unused media location in Settings before keeping another folder"
                );
                config.media_roots.push(canonical.clone());
            }
        }
        config.save(&self.config_dir)?;
        if favourite {
            self.boundary.keep_folder(canonical);
        }
        Ok(self.status())
    }
    pub fn plan(&mut self, inspection_id: &str, edits: Vec<Edit>) -> Result<Plan> {
        self.prune();
        ensure!(edits.len() <= 4096, "Too many proposed edits");
        let snapshot = self
            .snapshots
            .get(inspection_id)
            .context("Inspection expired; reopen the file")?;
        ensure!(
            snapshot.document.write_available,
            "In-place editing is unavailable for this file"
        );
        snapshot.identity.verify(
            &self
                .boundary
                .authorize(Path::new(&snapshot.document.path))?,
        )?;
        let mut targets = HashSet::new();
        let mut changes = vec![];
        for edit in edits {
            ensure!(
                targets.insert((edit.track_uid.clone(), edit.property)),
                "Duplicate property edits are not allowed"
            );
            let capability = if let Some(uid) = &edit.track_uid {
                &snapshot
                    .document
                    .tracks
                    .iter()
                    .find(|t| &t.uid == uid)
                    .context("Unknown track UID")?
                    .editable
            } else {
                &snapshot.document.editable
            };
            ensure!(
                capability.contains(&edit.property),
                "Requested property is not editable"
            );
            if let Some(change) = self.adapter.plan_change(&snapshot.document, edit)? {
                changes.push(change);
            }
        }
        self.store_plan(inspection_id.to_string(), changes)
    }
    fn store_plan(&mut self, inspection_id: String, changes: Vec<Change>) -> Result<Plan> {
        let document = &self
            .snapshots
            .get(&inspection_id)
            .context("Inspection expired")?
            .document;
        let path = document.path.clone();
        let mut warnings = self.adapter.validate_plan(document, &changes)?;
        warnings.insert(0, "Writes modify the original file in place. A metadata record is not a file backup and cannot repair an interrupted write.".into());
        ensure!(
            self.plans.len() < 16,
            "Too many staged plans; reopen the file or wait for old plans to expire"
        );
        let plan = Plan {
            id: uuid::Uuid::new_v4().to_string(),
            inspection_id,
            path,
            changes,
            warnings,
        };
        self.plans.insert(
            plan.id.clone(),
            Staged {
                plan: plan.clone(),
                created: Instant::now(),
            },
        );
        Ok(plan)
    }
    pub fn restore_plan(&mut self, operation_id: &str) -> Result<Plan> {
        let id = uuid::Uuid::parse_str(operation_id)
            .context("Invalid operation ID")?
            .to_string();
        let record_path = self
            .config_dir
            .join("operations")
            .join(format!("{id}.json"));
        ensure!(
            fs::metadata(&record_path)?.len() <= 16 * 1024 * 1024,
            "Operation record exceeds size limit"
        );
        let record: Record = serde_json::from_slice(&fs::read(record_path)?)?;
        ensure!(
            record.report.as_ref().is_some_and(|r| r.success),
            "Only a verified save can produce an automatic restoration plan"
        );
        let current = self.inspect(Path::new(&record.path))?;
        ensure!(current.write_available, "File is not writable");
        let mut changes = vec![];
        for change in record.changes {
            ensure!(
                self.adapter.property_value(
                    &current,
                    change.track_uid.as_deref(),
                    change.property
                )? == change.after,
                "Saved properties changed after this operation; restoration would overwrite newer metadata"
            );
            changes.push(Change {
                track_uid: change.track_uid,
                property: change.property,
                before: change.after,
                after: change.before,
            });
        }
        self.store_plan(current.id, changes)
    }
    pub fn rule_library(&self) -> Result<super::rules::model::RuleLibrary> {
        super::rules::RuleStore::new(&self.config_dir).load()
    }
    pub fn save_rule_library(
        &self,
        library: super::rules::model::RuleLibrary,
    ) -> Result<super::rules::model::RuleLibrary> {
        super::rules::RuleStore::new(&self.config_dir).save(library)
    }
    pub fn preview_rules(
        &mut self,
        request: super::rules::model::RulePreviewRequest,
    ) -> Result<super::rules::model::RulePreview> {
        self.prune();
        let snapshot = self
            .snapshots
            .get(&request.inspection_id)
            .context("Inspection expired; reopen the file")?;
        snapshot.identity.verify(
            &self
                .boundary
                .authorize(Path::new(&snapshot.document.path))?,
        )?;
        let library = request
            .test_library
            .clone()
            .map(Ok)
            .unwrap_or_else(|| self.rule_library())?;
        let preview = super::rules::evaluate(&snapshot.document, &library, &request)?;
        let mut changes = vec![];
        for proposal in &preview.proposals {
            if let Some(change) = self
                .adapter
                .plan_change(&snapshot.document, proposal.edit.clone())?
            {
                changes.push(change);
            }
        }
        if !changes.is_empty() {
            self.adapter.validate_plan(&snapshot.document, &changes)?;
        }
        Ok(preview)
    }
    pub fn save(&mut self, plan_id: &str) -> Result<SaveReport> {
        self.prune();
        // Plans are single use, including failed preflight attempts.
        let plan = self
            .plans
            .remove(plan_id)
            .context("Plan expired or was already used; preview the changes again")?
            .plan;
        ensure!(!plan.changes.is_empty(), "There are no changes to save");
        self.capture_write_sections(&plan.inspection_id)?;
        let snapshot = self
            .snapshots
            .get(&plan.inspection_id)
            .context("Inspection expired; reopen the file")?;
        ensure!(
            snapshot.document.write_available,
            "Writing is unavailable; refresh the metadata before saving"
        );
        let path = self.boundary.authorize(Path::new(&plan.path))?;
        let _operation = snapshot.identity.lock_operation()?;
        let _file_guard = hold_file_identity(&path)?;
        snapshot.identity.verify(&path)?;
        let access = preflight_write(&path)?;
        snapshot.identity.verify(&path)?;
        let original = snapshot.document.clone();
        let operation_id = uuid::Uuid::new_v4().to_string();
        let mut record = Record {
            id: operation_id.clone(),
            path: plan.path,
            changes: plan.changes.clone(),
            status: "started".into(),
            report: None,
        };
        self.record(&record)
            .context("Cannot capture original metadata; no write was started")?;
        // mkvpropedit needs its own write handle. File identity is checked immediately before launch.
        drop(access);
        snapshot.identity.verify(&self.boundary.authorize(&path)?)?;
        let started = Instant::now();
        let outcome = self.adapter.write(&path, &plan.changes);
        let mut report = SaveReport {
            operation_id,
            success: false,
            status: "verification_failed".into(),
            discrepancies: vec![],
            warnings: vec![],
            inspection: None,
            elapsed_ms: 0,
            restore_available: false,
        };
        let clean_exit = match outcome {
            Ok(output) => {
                if output.code != Some(0) || output.truncated {
                    report.warnings.push(format!(
                        "mkvpropedit exit {:?}; output truncated: {}. {} {}",
                        output.code,
                        output.truncated,
                        output.stderr,
                        String::from_utf8_lossy(&output.stdout)
                    ));
                }
                output.code == Some(0) && !output.truncated
            }
            Err(error) => {
                report.warnings.push(format!("Write process failed: {error:#}. The file may have changed; it will be reinspected."));
                false
            }
        };
        if !snapshot.identity.same_file(&path).unwrap_or(false) {
            report
                .discrepancies
                .push("File identity changed during the save".into());
        }
        // Always reinspect, even after a nonzero process exit. Never report an exit code alone as success.
        match self
            .boundary
            .authorize(&path)
            .and_then(|authorized| self.adapter.inspect_verification(&authorized))
        {
            Ok(mut after) => {
                report
                    .discrepancies
                    .extend(self.adapter.verify(&original, &after, &plan.changes));
                if clean_exit && report.discrepancies.is_empty() {
                    self.adapter.retain_technical(&original, &mut after);
                }
                if after.mediainfo.is_none()
                    && after
                        .tracks
                        .iter()
                        .any(|t| matches!(t.kind.as_str(), "audio" | "video"))
                {
                    after.analysis.status = "pending".into();
                }
                report.inspection = Some(after);
            }
            Err(error) => report
                .discrepancies
                .push(format!("Post-write inspection failed: {error:#}")),
        }
        report.success = clean_exit && report.discrepancies.is_empty();
        report.status = if report.success {
            "verified"
        } else if !clean_exit {
            "write_failed_or_warned"
        } else {
            "verification_failed"
        }
        .into();
        report.elapsed_ms = started.elapsed().as_millis() as u64;
        report.restore_available = report.success;
        // Invalidate every stale inspection/plan of this file, not only the submitted plan.
        self.snapshots
            .retain(|_, s| Path::new(&s.document.path) != path);
        self.plans.retain(|_, p| Path::new(&p.plan.path) != path);
        if let Some(document) = &report.inspection
            && let Ok(identity) = Identity::capture(&path)
        {
            self.snapshots.insert(
                document.id.clone(),
                Snapshot {
                    document: document.clone(),
                    identity,
                    created: Instant::now(),
                    analysis_running: false,
                    cancel: Arc::new(AtomicBool::new(false)),
                },
            );
        }
        record.status = report.status.clone();
        record.report = Some(report.clone());
        if let Err(error) = self.record(&record) {
            report.warnings.push(format!("Metadata was processed, but the final operation record could not be saved: {error:#}"));
            report.restore_available = false;
        }
        Ok(report)
    }
    fn record(&self, record: &Record) -> Result<()> {
        let folder = self.config_dir.join("operations");
        fs::create_dir_all(&folder)?;
        let mut temp = tempfile::NamedTempFile::new_in(&folder)?;
        serde_json::to_writer_pretty(&mut temp, record)?;
        temp.write_all(b"\n")?;
        temp.as_file().sync_all()?;
        temp.persist(folder.join(format!("{}.json", record.id)))?;
        Ok(())
    }

    fn capture_write_sections(&mut self, id: &str) -> Result<()> {
        let snapshot = self.snapshots.get(id).context("Inspection expired")?;
        if snapshot.document.chapters.available && snapshot.document.tags.available {
            return Ok(());
        }
        let path = self
            .boundary
            .authorize(Path::new(&snapshot.document.path))?;
        snapshot.identity.verify(&path)?;
        let mut document = snapshot.document.clone();
        self.adapter
            .complete_sections(&mut document, &AtomicBool::new(false));
        snapshot.identity.verify(&self.boundary.authorize(&path)?)?;
        ensure!(
            document.write_available,
            "Chapter/tag preservation metadata unavailable; no write was started"
        );
        self.snapshots.get_mut(id).unwrap().document = document;
        Ok(())
    }
}
