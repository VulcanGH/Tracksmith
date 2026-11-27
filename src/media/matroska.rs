use super::adapter::{AdapterStatus, ContainerAdapter};
use super::{filesystem::Identity, technical_cache::TechnicalCache};
use super::{model::*, process};
use crate::config::MediaTools;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    ffi::OsString,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub struct Matroska {
    merge: Option<PathBuf>,
    extract: Option<PathBuf>,
    writer: Option<PathBuf>,
    mediainfo: Option<PathBuf>,
    dll: Option<PathBuf>,
    ffprobe: Option<PathBuf>,
    supported: HashSet<String>,
    cache: TechnicalCache,
    analysis_gate: Mutex<()>,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AnalysisRequest {
    Skip,
    Cached,
    Complete,
    Full,
}

const PRIMARY_ANALYSIS_TIMEOUT: Duration = Duration::from_secs(8);
const AUDIO_SAMPLE_SECONDS: u32 = 30;
const AUDIO_SAMPLE_LIMIT: u64 = 128 * 1024 * 1024;
fn discover(configured: &Option<PathBuf>, folder: &str, file: &str) -> Option<PathBuf> {
    if let Some(path) = configured {
        return path.is_file().then(|| path.clone());
    }
    let known = PathBuf::from(
        std::env::var_os("ProgramFiles").unwrap_or_else(|| "C:\\Program Files".into()),
    )
    .join(folder)
    .join(file);
    if known.is_file() {
        return Some(known);
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|p| p.join(file))
            .find(|p| p.is_absolute() && p.is_file())
    })
}
impl Matroska {
    pub fn new(tools: &MediaTools, config_dir: &Path) -> Self {
        let mut adapter = Self {
            merge: discover(&tools.mkvmerge, "MKVToolNix", "mkvmerge.exe"),
            extract: discover(&tools.mkvextract, "MKVToolNix", "mkvextract.exe"),
            writer: discover(&tools.mkvpropedit, "MKVToolNix", "mkvpropedit.exe"),
            // Do not auto-select the Windows GUI executable as a command-line utility.
            mediainfo: tools.mediainfo.clone().filter(|p| p.is_file()),
            dll: discover(&tools.mediainfo_dll, "MediaInfo", "MediaInfo.dll"),
            ffprobe: discover(&tools.ffprobe, "ffmpeg/bin", "ffprobe.exe"),
            supported: HashSet::new(),
            cache: TechnicalCache::new(config_dir),
            analysis_gate: Mutex::new(()),
            diagnostics: vec![],
        };
        if let Some(writer) = &adapter.writer {
            match process::run(writer, &["--list-property-names".into()], false)
                .and_then(|o| o.checked())
            {
                Ok(output) => {
                    for line in String::from_utf8_lossy(&output).lines() {
                        if let Some((name, _)) = line.split_once('|') {
                            adapter.supported.insert(name.trim().into());
                        }
                    }
                }
                Err(error) => {
                    adapter
                        .diagnostics
                        .push(format!("Writer capability discovery failed: {error:#}"));
                    adapter.writer = None;
                }
            }
        }
        for (available, message) in [
            (
                adapter.merge.is_some(),
                "mkvmerge is unavailable; configure tools.mkvmerge",
            ),
            (
                adapter.extract.is_some(),
                "mkvextract is unavailable; chapters/tags cannot be inspected and writes are disabled",
            ),
            (
                adapter.writer.is_some(),
                "mkvpropedit is unavailable; editing is disabled",
            ),
            (
                adapter.ffprobe.is_some() || adapter.mediainfo.is_some() || adapter.dll.is_some(),
                "No technical provider is available; configure tools.ffprobe",
            ),
        ] {
            if !available {
                adapter.diagnostics.push(message.into());
            }
        }
        if adapter.ffprobe.is_none() && (adapter.mediainfo.is_some() || adapter.dll.is_some()) {
            adapter.diagnostics.push("FFprobe is unavailable; audio uses the slower MediaInfo compatibility path. Configure tools.ffprobe for fast audio checks.".into());
        }
        adapter
    }
    pub fn inspection_available(&self) -> bool {
        self.merge.is_some()
    }
    pub fn writing_available(&self) -> bool {
        self.merge.is_some() && self.extract.is_some() && self.writer.is_some()
    }
    fn section(&self, path: &Path, mode: &str, cancel: &AtomicBool) -> MetadataSection {
        let result = (|| -> Result<(String, Option<XmlElement>)> {
            let tool = self
                .extract
                .as_ref()
                .context("mkvextract is not configured")?;
            let bytes = process::run_cancellable(
                tool,
                &[path.as_os_str().into(), mode.into()],
                false,
                Duration::from_secs(90),
                Some(cancel),
            )?
            .checked()?;
            let xml = String::from_utf8(bytes)?;
            if xml.trim().is_empty() {
                return Ok((xml, None));
            }
            let doc = roxmltree::Document::parse(&xml).context("Malformed metadata XML")?;
            ensure!(
                doc.descendants().count() <= 100_000,
                "Metadata XML contains too many nodes"
            );
            fn element(node: roxmltree::Node<'_, '_>, depth: usize) -> Result<XmlElement> {
                ensure!(depth <= 64, "Metadata XML nesting exceeds 64 levels");
                Ok(XmlElement {
                    name: node.tag_name().name().into(),
                    text: node
                        .text()
                        .filter(|v| !v.trim().is_empty())
                        .map(str::to_string),
                    attributes: node
                        .attributes()
                        .map(|a| (a.name().into(), a.value().into()))
                        .collect(),
                    children: node
                        .children()
                        .filter(|n| n.is_element())
                        .map(|n| element(n, depth + 1))
                        .collect::<Result<_>>()?,
                })
            }
            let tree = element(doc.root_element(), 0)?;
            Ok((xml, Some(tree)))
        })();
        match result {
            Ok((xml, tree)) => MetadataSection {
                available: true,
                xml: Some(xml),
                tree,
                error: None,
            },
            Err(error) => MetadataSection {
                available: false,
                xml: None,
                tree: None,
                error: Some(format!("{error:#}")),
            },
        }
    }
    fn enrich(&self, path: &Path, timeout: Duration, cancel: &AtomicBool) -> Result<Value> {
        let output = if let Some(cli) = &self.mediainfo {
            process::run_cancellable(
                cli,
                &[
                    "--Output=JSON".into(),
                    "--Full".into(),
                    path.as_os_str().into(),
                ],
                false,
                timeout,
                Some(cancel),
            )?
        } else {
            let dll = self.dll.as_ref().context("MediaInfo is not configured")?;
            process::run_cancellable(
                &std::env::current_exe()?,
                &[
                    "--mediainfo-worker".into(),
                    dll.as_os_str().into(),
                    path.as_os_str().into(),
                ],
                false,
                timeout,
                Some(cancel),
            )?
        };
        Ok(serde_json::from_slice(&output.checked()?)?)
    }

    fn provider_key(&self) -> String {
        let fingerprints: Vec<_> = [&self.ffprobe, &self.mediainfo, &self.dll]
            .iter()
            .map(|path| {
                let metadata = path.as_ref().and_then(|p| std::fs::metadata(p).ok());
                format!(
                    "{path:?}:{:?}:{:?}",
                    metadata.as_ref().map(|m| m.len()),
                    metadata.and_then(|m| m.modified().ok())
                )
            })
            .collect();
        format!(
            "audio-first-v2:{}:{fingerprints:?}",
            env!("CARGO_PKG_VERSION")
        )
    }

    fn cached_facts(&self, path: &Path) -> Option<Value> {
        match self.cache.load(path, &self.provider_key()) {
            Ok(facts) => facts,
            Err(error) => {
                tracing::warn!(%error, "Ignoring unavailable technical cache");
                None
            }
        }
    }

    fn cache_facts(&self, document: &Inspection, stamp: &str) {
        let required = if matches!(document.analysis.scope.as_str(), "audio_sample" | "audio") {
            "audio"
        } else {
            ""
        };
        let verified = document
            .tracks
            .iter()
            .filter(|t| {
                matches!(t.kind.as_str(), "audio" | "video")
                    && (required.is_empty() || t.kind == required)
            })
            .all(|t| t.mapping == "uid_number_type_verified");
        if verified
            && let Some(facts) = &document.mediainfo
            && let Err(error) = self.cache.store(
                Path::new(&document.path),
                &self.provider_key(),
                facts,
                stamp,
            )
        {
            tracing::warn!(%error, "Technical facts could not be cached");
        }
    }

    fn audio_sample(&self, path: &Path, source: &Value, cancel: &AtomicBool) -> Result<Value> {
        let ids = source["tracks"]
            .as_array()
            .context("Missing tracks")?
            .iter()
            .filter(|t| t["type"] == "audio")
            .map(|t| {
                t["id"]
                    .as_u64()
                    .context("Invalid audio ID")
                    .map(|id| id.to_string())
            })
            .collect::<Result<Vec<_>>>()?;
        ensure!(!ids.is_empty(), "No audio tracks to analyse");
        let temporary = tempfile::tempdir().context("Creating audio analysis workspace")?;
        let sample = temporary.path().join("audio.mka");
        let tool = self.merge.as_ref().context("mkvmerge is not configured")?;
        process::run_cancellable(
            tool,
            &[
                "-o".into(),
                sample.as_os_str().into(),
                "--split".into(),
                format!("parts:00:00:00-00:00:{AUDIO_SAMPLE_SECONDS}").into(),
                "--no-video".into(),
                "--no-subtitles".into(),
                "--no-buttons".into(),
                "--no-chapters".into(),
                "--no-attachments".into(),
                "--no-global-tags".into(),
                "--no-track-tags".into(),
                "--audio-tracks".into(),
                ids.join(",").into(),
                path.as_os_str().into(),
            ],
            false,
            Duration::from_secs(15),
            Some(cancel),
        )?
        .checked()?;
        ensure!(
            std::fs::metadata(&sample)?.len() <= AUDIO_SAMPLE_LIMIT,
            "Audio sample exceeds 128 MiB"
        );
        let identified: Value = serde_json::from_slice(
            &process::run_cancellable(
                tool,
                &["-J".into(), sample.as_os_str().into()],
                false,
                Duration::from_secs(15),
                Some(cancel),
            )?
            .checked()?,
        )?;
        let media = self.enrich(&sample, Duration::from_secs(15), cancel)?;
        audio_sample_info(source, &identified, &media)
    }

    fn inspect_metadata(
        &self,
        path: &Path,
        full: bool,
        analysis: AnalysisRequest,
        cancel: &AtomicBool,
    ) -> Result<Inspection> {
        let started = Instant::now();
        let extension = path
            .extension()
            .and_then(|v| v.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        ensure!(
            matches!(extension.as_str(), "mkv" | "mka"),
            "Only MKV and MKA are supported in this phase"
        );
        let bytes = process::run(
            self.merge.as_ref().context("mkvmerge is not configured")?,
            &["-J".into(), path.as_os_str().into()],
            false,
        )?
        .checked()?;
        let mut raw: Value =
            serde_json::from_slice(&bytes).context("Malformed mkvmerge identification")?;
        ensure!(
            raw["container"]["recognized"] == true
                && raw["container"]["supported"] == true
                && raw["container"]["type"] == "Matroska",
            "File is not a supported Matroska container"
        );
        let raw_tracks = raw["tracks"]
            .as_array()
            .context("Missing Matroska tracks")?;
        ensure!(
            !raw_tracks.is_empty() && raw_tracks.len() <= 1024,
            "Track count is missing or exceeds the inspection limit"
        );
        let mut warnings = self.diagnostics.clone();
        for warning in raw["warnings"].as_array().into_iter().flatten() {
            warnings.push(warning.to_string());
        }
        let media = if analysis != AnalysisRequest::Skip {
            self.cached_facts(path)
        } else {
            None
        };
        let mut identities = HashSet::new();
        let mut numbers = HashSet::new();
        let mut ids = HashSet::new();
        let mut tracks = Vec::new();
        for track in raw_tracks {
            let properties = &track["properties"];
            let uid = number(&properties["uid"]).context("Track UID is missing")?;
            ensure!(
                uid.parse::<u64>()? != 0 && identities.insert(uid.clone()),
                "Track UIDs are zero or ambiguous"
            );
            let track_number = number(&properties["number"]).context("Track number is missing")?;
            ensure!(
                track_number.parse::<u64>()? != 0 && numbers.insert(track_number.clone()),
                "Track numbers are zero or ambiguous"
            );
            let id = u32::try_from(track["id"].as_u64().context("Invalid mkvmerge track ID")?)?;
            ensure!(ids.insert(id), "mkvmerge track IDs are ambiguous");
            let kind = track["type"].as_str().unwrap_or("unknown").to_string();
            let flags: BTreeMap<_, _> = Property::ALL
                .into_iter()
                .filter(|p| !matches!(p, Property::Title | Property::Language))
                .map(|p| {
                    (
                        serde_json::to_value(p)
                            .unwrap()
                            .as_str()
                            .unwrap()
                            .to_string(),
                        properties[p.json_key()].as_bool(),
                    )
                })
                .collect();
            let mut technical = properties.clone();
            json_safe(&mut technical);
            tracks.push(Track {
                id,
                uid,
                number: track_number,
                kind,
                codec: track["codec"].as_str().unwrap_or("Unknown").into(),
                title: properties["track_name"].as_str().map(str::to_owned),
                language: properties["language"].as_str().map(str::to_owned),
                language_ietf: properties["language_ietf"].as_str().map(str::to_owned),
                editable: Property::ALL
                    .into_iter()
                    .filter(|p| {
                        self.supported.contains(p.tool_name(false))
                            && (*p != Property::Language
                                || self.supported.contains("language-ietf"))
                    })
                    .collect(),
                flags,
                technical,
                mediainfo: None,
                mapping: "not_requested".into(),
            });
        }
        // Preview needs track facts only. Full inspection at save time still
        // captures chapters/tags for preservation and post-write verification.
        let section = |mode| {
            if full {
                self.section(path, mode, cancel)
            } else {
                MetadataSection {
                    available: false,
                    xml: None,
                    tree: None,
                    error: Some(
                        if analysis == AnalysisRequest::Cached {
                            "Loading container details"
                        } else {
                            "Deferred until save"
                        }
                        .into(),
                    ),
                }
            }
        };
        let chapters = section("chapters");
        let tags = section("tags");
        let write_available = self.writing_available()
            && (!full || (chapters.available && tags.available))
            && !std::fs::metadata(path)?.permissions().readonly();
        if !write_available {
            for track in &mut tracks {
                track.editable.clear();
            }
        }
        json_safe(&mut raw);
        let mut document = Inspection {
            id: uuid::Uuid::new_v4().to_string(),
            path: path.display().to_string(),
            size_bytes: std::fs::metadata(path)?.len().to_string(),
            container: "Matroska".into(),
            title: raw["container"]["properties"]["title"]
                .as_str()
                .map(str::to_owned),
            editable: if write_available && self.supported.contains("title") {
                vec![Property::Title]
            } else {
                vec![]
            },
            tracks,
            attachments: raw["attachments"].clone(),
            chapters,
            tags,
            sections_pending: !full,
            container_info: raw["container"].clone(),
            identification: raw,
            mediainfo: None,
            analysis: TechnicalState {
                status: if analysis == AnalysisRequest::Skip {
                    "not_requested"
                } else {
                    "pending"
                }
                .into(),
                scope: "none".into(),
                cached: false,
                error: None,
            },
            warnings,
            write_available,
            elapsed_ms: started.elapsed().as_millis() as u64,
        };
        if let Some(media) = media {
            attach_technical(&mut document, media, true);
        }
        if !document
            .tracks
            .iter()
            .any(|t| matches!(t.kind.as_str(), "audio" | "video"))
        {
            document.analysis.status = "ready".into();
        }
        if ((analysis == AnalysisRequest::Complete || analysis == AnalysisRequest::Full)
            && document.analysis.status == "pending")
            || (analysis == AnalysisRequest::Full && document.analysis.scope != "full")
        {
            self.complete_analysis(document, analysis == AnalysisRequest::Full, cancel)
        } else {
            Ok(document)
        }
    }
}

fn technical_signature(raw: &Value) -> Value {
    let mut tracks = raw["tracks"].clone();
    for track in tracks.as_array_mut().into_iter().flatten() {
        if let Some(properties) = track["properties"].as_object_mut() {
            for property in Property::ALL {
                properties.remove(property.json_key());
            }
            properties.remove("language_ietf");
        }
    }
    tracks
}

fn technical_facts(mut media: Value) -> Value {
    let tracks = media["media"]["track"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let tracks: Vec<_> = tracks
        .into_iter()
        .filter_map(|mut track| {
            if !matches!(track["@type"].as_str(), Some("Audio" | "Video" | "Text")) {
                return None;
            }
            if let Some(fields) = track.as_object_mut() {
                fields.retain(|key, _| {
                    !["Title", "Language", "Default", "Forced"]
                        .iter()
                        .any(|prefix| key.starts_with(prefix))
                });
            }
            Some(track)
        })
        .collect();
    media = json!({"media":{"track":tracks}, "tracksmith_analysis":{"scope":"full"}});
    json_safe(&mut media);
    media
}

fn attach_technical(document: &mut Inspection, mut media: Value, cached: bool) {
    json_safe(&mut media);
    let audio_sample = media["tracksmith_analysis"]["scope"] == "audio_sample";
    let audio_only = audio_sample || media["tracksmith_analysis"]["scope"] == "audio";
    let scope = if audio_sample {
        "audio_sample"
    } else if audio_only {
        "audio"
    } else {
        "full"
    };
    let mut unconfirmed = false;
    for track in &mut document.tracks {
        let expected = match track.kind.as_str() {
            "audio" => "Audio",
            "video" => "Video",
            "subtitles" => "Text",
            _ => "Other",
        };
        let facts = media["media"]["track"].as_array().into_iter().flatten();
        let candidates: Vec<_> = facts
            .clone()
            .filter(|v| number(&v["ID"]).as_ref() == Some(&track.number) && v["@type"] == expected)
            .collect();
        let uid_matches = facts
            .filter(|v| number(&v["UniqueID"]).as_ref() == Some(&track.uid))
            .count();
        let verified = candidates.len() == 1
            && uid_matches == 1
            && number(&candidates[0]["UniqueID"]).as_ref() == Some(&track.uid);
        track.mediainfo = verified.then(|| candidates[0].clone());
        track.mapping = if verified {
            "uid_number_type_verified"
        } else {
            "unavailable"
        }
        .into();
        if matches!(track.kind.as_str(), "audio" | "video")
            && (!audio_only || track.kind == "audio")
            && (!verified
                || track
                    .mediainfo
                    .as_ref()
                    .is_some_and(|v| v["Tracksmith_Incomplete"] == true))
        {
            unconfirmed = true;
        }
    }
    let error = media["tracksmith_analysis"]["warning"].as_str().map(str::to_owned)
        .or_else(|| unconfirmed.then(|| "Some technical track identities could not be confirmed; affected technical naming rules are skipped.".into()));
    if let Some(message) = &error
        && !document.warnings.contains(message)
    {
        document.warnings.push(message.clone());
    }
    document.analysis = TechnicalState {
        status: if audio_sample || unconfirmed {
            "partial"
        } else {
            "ready"
        }
        .into(),
        scope: scope.into(),
        cached,
        error,
    };
    document.mediainfo = Some(media);
}

/// Match source UIDs and sample numbers before remapping sampled facts.
fn audio_sample_info(source: &Value, sample: &Value, media: &Value) -> Result<Value> {
    let originals = source["tracks"]
        .as_array()
        .context("Missing source tracks")?;
    let samples = sample["tracks"]
        .as_array()
        .context("Missing sample tracks")?;
    let mut bindings = BTreeMap::new();
    for track in samples {
        ensure!(track["type"] == "audio", "Non-audio track in audio sample");
        let uid = number(&track["properties"]["uid"]).context("Sample UID missing")?;
        let matches: Vec<_> = originals
            .iter()
            .filter(|t| {
                t["type"] == "audio" && number(&t["properties"]["uid"]).as_ref() == Some(&uid)
            })
            .collect();
        ensure!(
            matches.len() == 1,
            "Audio sample UID does not uniquely match its source"
        );
        ensure!(
            bindings
                .insert(
                    uid,
                    (
                        number(&track["properties"]["number"])
                            .context("Sample track number missing")?,
                        number(&matches[0]["properties"]["number"])
                            .context("Source track number missing")?,
                    )
                )
                .is_none(),
            "Duplicate sample UID"
        );
    }
    let mut tracks = Vec::new();
    let mut seen = HashSet::new();
    for track in media["media"]["track"]
        .as_array()
        .context("Missing MediaInfo tracks")?
    {
        if track["@type"] != "Audio" {
            continue;
        }
        let uid = number(&track["UniqueID"]).context("MediaInfo audio UID missing")?;
        let (sample_number, source_number) =
            bindings.get(&uid).context("Unmapped sample audio UID")?;
        ensure!(
            number(&track["ID"]).as_ref() == Some(sample_number) && seen.insert(uid),
            "MediaInfo audio sample identity is ambiguous"
        );
        let mut track = track.clone();
        track["ID"] = Value::String(source_number.clone());
        // Sample statistics are not whole-file duration, size or average bitrate.
        track
            .as_object_mut()
            .context("Invalid audio facts")?
            .retain(|key, _| {
                ![
                    "Duration",
                    "StreamSize",
                    "FrameCount",
                    "SamplingCount",
                    "BitRate",
                ]
                .iter()
                .any(|prefix| key.starts_with(prefix))
            });
        tracks.push(track);
    }
    ensure!(
        !tracks.is_empty(),
        "No audio facts were read from the analysis sample"
    );
    Ok(json!({"media": {"track": tracks}, "tracksmith_analysis": {
        "scope": "audio_sample", "sample_seconds": AUDIO_SAMPLE_SECONDS
    }}))
}

fn number(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_u64().map(|n| n.to_string()))
}
/// Preserve large identifiers in all raw metadata, including unknown future properties.
fn json_safe(value: &mut Value) {
    match value {
        Value::Number(n) if n.as_u64().is_some_and(|v| v > 9_007_199_254_740_991) => {
            *value = Value::String(n.to_string())
        }
        Value::Array(values) => {
            for item in values {
                json_safe(item);
            }
        }
        Value::Object(map) => {
            for (key, value) in map {
                if key.to_ascii_lowercase().contains("uid") && value.is_number() {
                    *value = Value::String(value.to_string());
                } else {
                    json_safe(value);
                }
            }
        }
        _ => (),
    }
}

impl ContainerAdapter for Matroska {
    fn invalidate_technical(&self, path: &Path) -> Result<()> {
        self.cache.invalidate(path)
    }
    fn status(&self) -> AdapterStatus {
        AdapterStatus {
            inspection_available: self.inspection_available(),
            writing_available: self.writing_available(),
            diagnostics: self.diagnostics.clone(),
        }
    }
    fn property_value(
        &self,
        document: &Inspection,
        uid: Option<&str>,
        property: Property,
    ) -> Result<Value> {
        value(document, uid, property)
    }
    fn plan_change(&self, document: &Inspection, edit: Edit) -> Result<Option<Change>> {
        let before = value(document, edit.track_uid.as_deref(), edit.property)?;
        let after = match edit.property {
            Property::Title => {
                ensure!(
                    edit.value.is_null()
                        || edit
                            .value
                            .as_str()
                            .is_some_and(|s| s.len() <= 4096 && !s.contains('\0')),
                    "Title must be text of at most 4096 UTF-8 bytes or null"
                );
                edit.value
            }
            Property::Language => normalize_language(
                edit.value
                    .as_str()
                    .context("Language must be a BCP 47 tag; use und for undefined")?,
            )?,
            _ => {
                ensure!(edit.value.is_boolean(), "Flag values must be boolean");
                edit.value
            }
        };
        Ok((before != after).then_some(Change {
            track_uid: edit.track_uid,
            property: edit.property,
            before,
            after,
        }))
    }
    fn validate_plan(&self, document: &Inspection, changes: &[Change]) -> Result<Vec<String>> {
        ensure!(document.write_available, "In-place editing is unavailable");
        for change in changes {
            let capabilities = if let Some(uid) = &change.track_uid {
                &document
                    .tracks
                    .iter()
                    .find(|t| &t.uid == uid)
                    .context("Unknown track UID")?
                    .editable
            } else {
                &document.editable
            };
            ensure!(
                capabilities.contains(&change.property),
                "Requested property is not editable"
            );
        }
        command_args(Path::new(&document.path), changes)?;
        Ok(changes.iter().filter(|change| change.property == Property::Language && change.after["legacy"] == "und" && change.after["ietf"].as_str().is_some_and(|s| s != "und"))
            .map(|_| "This BCP 47 language has no matching ISO 639-2 code. Its IETF tag is preserved; the legacy language field will be und (unspecified).".to_string()).collect())
    }
    fn inspect_quick(&self, path: &Path) -> Result<Inspection> {
        self.inspect_metadata(
            path,
            false,
            AnalysisRequest::Cached,
            &AtomicBool::new(false),
        )
    }
    fn complete_sections(&self, document: &mut Inspection, cancel: &AtomicBool) {
        let path = Path::new(&document.path);
        if !document.chapters.available {
            document.chapters = self.section(path, "chapters", cancel);
        }
        if !document.tags.available {
            document.tags = self.section(path, "tags", cancel);
        }
        document.sections_pending = false;
        document.write_available = self.writing_available()
            && document.chapters.available
            && document.tags.available
            && std::fs::metadata(path).is_ok_and(|m| !m.permissions().readonly());
        if !document.write_available {
            document.editable.clear();
            for track in &mut document.tracks {
                track.editable.clear();
            }
        }
    }
    fn inspect_verification(&self, path: &Path) -> Result<Inspection> {
        self.inspect_metadata(path, true, AnalysisRequest::Skip, &AtomicBool::new(false))
    }
    fn retain_technical(&self, before: &Inspection, after: &mut Inspection) {
        if technical_signature(&before.identification) == technical_signature(&after.identification)
            && let Some(facts) = &before.mediainfo
            && facts["tracksmith_analysis"]["provider"].as_str()
                == Some(self.provider_key().as_str())
            && let Ok(identity) = Identity::capture(Path::new(&after.path))
        {
            attach_technical(after, facts.clone(), true);
            if let Ok(stamp) = identity.cache_stamp() {
                self.cache_facts(after, &stamp);
            }
        }
    }
    fn complete_analysis(
        &self,
        mut document: Inspection,
        retry: bool,
        cancel: &AtomicBool,
    ) -> Result<Inspection> {
        let path = PathBuf::from(&document.path);
        if retry && let Some(previous) = document.analysis.error.take() {
            document.warnings.retain(|warning| warning != &previous);
        }
        let identity = Identity::capture(&path)?;
        // Serialise native analysis, not the engine. Recheck the cache after waiting
        // so duplicate opens of the same file do not run the parser twice.
        let _slot = self
            .analysis_gate
            .lock()
            .map_err(|_| anyhow::anyhow!("Technical worker unavailable"))?;
        identity.verify(&path)?;
        ensure!(
            !cancel.load(Ordering::SeqCst),
            "Technical analysis cancelled"
        );
        if !retry && let Some(facts) = self.cached_facts(&path) {
            attach_technical(&mut document, facts, true);
            return Ok(document);
        }
        let started = Instant::now();
        let provider = self.provider_key();
        let timeout = if retry {
            Duration::from_secs(90)
        } else {
            PRIMARY_ANALYSIS_TIMEOUT
        };
        let fast_audio = !retry && self.ffprobe.is_some();
        let result = if fast_audio {
            super::audio_probe::inspect(self.ffprobe.as_ref().unwrap(), &document, cancel)
        } else {
            self.enrich(&path, timeout, cancel)
        };
        ensure!(
            !cancel.load(Ordering::SeqCst),
            "Technical analysis cancelled"
        );
        let result = match result {
            Ok(facts) if fast_audio => Ok(facts),
            Ok(facts) => Ok(technical_facts(facts)),
            Err(primary) => {
                if !fast_audio
                    && (self.mediainfo.is_some() || self.dll.is_some())
                    && document.tracks.iter().any(|t| t.kind == "audio")
                {
                    self.audio_sample(&path, &document.identification, cancel).map(|mut facts| {
                        facts["tracksmith_analysis"]["warning"] = json!(format!("Full technical analysis unavailable: {primary:#}. Audio facts come from the first {AUDIO_SAMPLE_SECONDS} seconds; video facts are unavailable."));
                        facts
                    }).map_err(|fallback| anyhow::anyhow!("{primary:#}; audio fallback failed: {fallback:#}"))
                } else {
                    Err(primary)
                }
            }
        };
        identity.verify(&path)?;
        ensure!(
            self.provider_key() == provider,
            "Metadata provider changed during analysis; retry"
        );
        match result {
            Ok(mut facts) => {
                facts["tracksmith_analysis"]["provider"] = json!(provider);
                attach_technical(&mut document, facts, false);
                self.cache_facts(&document, &identity.cache_stamp()?);
            }
            Err(error) => {
                let message = format!(
                    "Technical analysis unavailable: {error:#}. Existing technical titles are preserved."
                );
                document.analysis = TechnicalState {
                    status: "failed".into(),
                    scope: "none".into(),
                    cached: false,
                    error: Some(message.clone()),
                };
                document.warnings.push(message);
                for track in &mut document.tracks {
                    track.mapping = "unavailable".into();
                    track.mediainfo = None;
                }
                document.mediainfo = None;
            }
        }
        document.elapsed_ms += started.elapsed().as_millis() as u64;
        Ok(document)
    }
    fn inspect_batch(
        &self,
        path: &Path,
        enrich: bool,
        full: bool,
        cancel: &AtomicBool,
    ) -> Result<Inspection> {
        self.inspect_metadata(
            path,
            false,
            if enrich && full {
                AnalysisRequest::Full
            } else if enrich {
                AnalysisRequest::Complete
            } else {
                AnalysisRequest::Skip
            },
            cancel,
        )
    }
    fn write(&self, path: &Path, changes: &[Change]) -> Result<process::Output> {
        let args = command_args(path, changes)?;
        process::run(
            self.writer.as_ref().context("mkvpropedit is unavailable")?,
            &args,
            true,
        )
    }
    fn verify(&self, before: &Inspection, after: &Inspection, changes: &[Change]) -> Vec<String> {
        let mut discrepancies = vec![];
        for change in changes {
            match value(after, change.track_uid.as_deref(), change.property) {
                Ok(actual) if actual == change.after => (),
                Ok(actual) => discrepancies.push(format!(
                    "{:?} on {:?}: requested {}, read {}",
                    change.property, change.track_uid, change.after, actual
                )),
                Err(error) => discrepancies.push(error.to_string()),
            }
        }
        let mut expected = before.identification.clone();
        for change in changes {
            if let Some(uid) = &change.track_uid {
                if let Some(track) = expected["tracks"].as_array_mut().and_then(|tracks| {
                    tracks
                        .iter_mut()
                        .find(|t| number(&t["properties"]["uid"]).as_ref() == Some(uid))
                }) {
                    if change.property == Property::Language {
                        update(
                            &mut track["properties"],
                            "language",
                            &change.after["legacy"],
                        );
                        update(
                            &mut track["properties"],
                            "language_ietf",
                            &change.after["ietf"],
                        );
                    } else {
                        update(
                            &mut track["properties"],
                            change.property.json_key(),
                            &change.after,
                        );
                    }
                }
            } else {
                update(
                    &mut expected["container"]["properties"],
                    "title",
                    &change.after,
                );
            }
        }
        let mut actual = after.identification.clone();
        // File size may change when metadata grows; payload data is never rewritten as a fallback.
        for raw in [&mut expected, &mut actual] {
            if let Some(props) = raw["container"]["properties"].as_object_mut() {
                props.remove("file_size");
            }
        }
        if expected != actual {
            discrepancies.push("Unrequested identification metadata or structure changed".into());
            for key in [
                "tracks",
                "attachments",
                "container",
                "chapters",
                "global_tags",
                "track_tags",
            ] {
                if expected[key] != actual[key] {
                    discrepancies.push(format!("Identification section differs: {key}; compare the operation record with a fresh inspection"));
                }
            }
        }
        if serde_json::to_value(&before.chapters.tree).ok()
            != serde_json::to_value(&after.chapters.tree).ok()
            || !after.chapters.available
        {
            discrepancies.push("Chapters/editions could not be verified unchanged".into());
        }
        if serde_json::to_value(&before.tags.tree).ok()
            != serde_json::to_value(&after.tags.tree).ok()
            || !after.tags.available
        {
            discrepancies.push("Tags could not be verified unchanged".into());
        }
        discrepancies
    }
}
fn update(object: &mut Value, key: &str, value: &Value) {
    if let Some(map) = object.as_object_mut() {
        if value.is_null() {
            map.remove(key);
        } else {
            map.insert(key.into(), value.clone());
        }
    }
}
pub fn value(doc: &Inspection, uid: Option<&str>, property: Property) -> Result<Value> {
    let Some(uid) = uid else {
        ensure!(
            property == Property::Title,
            "Only the container title is editable"
        );
        return Ok(doc.title.clone().map(Value::String).unwrap_or(Value::Null));
    };
    let track = doc
        .tracks
        .iter()
        .find(|t| t.uid == uid)
        .context("Track UID is not present in this inspection")?;
    if property == Property::Language {
        Ok(json!({ "legacy": track.language, "ietf": track.language_ietf }))
    } else {
        Ok(track.technical[property.json_key()].clone())
    }
}
pub fn normalize_language(input: &str) -> Result<Value> {
    ensure!(
        input.len() <= 128 && !input.contains('\0'),
        "Invalid language tag"
    );
    let parsed = language_tags::LanguageTag::parse(input)
        .context("Use a BCP 47 language tag such as en, en-GB or und")?;
    parsed
        .validate()
        .context("Language tag is not registered or valid")?;
    let canonical = parsed.canonicalize()?.to_string();
    let primary = canonical.split('-').next().unwrap_or("und");
    // ISO 639-3 is not interchangeable with Matroska's ISO 639-2 legacy field.
    let legacy = include_str!("../../data/iso639-2.txt")
        .lines()
        .find_map(|line| {
            let columns: Vec<_> = line.split('|').collect();
            columns
                .iter()
                .any(|code| !code.is_empty() && *code == primary)
                .then_some(columns[0])
        })
        .unwrap_or("und");
    Ok(json!({ "legacy": legacy, "ietf": canonical }))
}
pub fn command_args(path: &Path, changes: &[Change]) -> Result<Vec<OsString>> {
    let mut args: Vec<OsString> = vec![
        path.as_os_str().into(),
        "--parse-mode".into(),
        "fast".into(),
        "--flush-on-close".into(),
    ];
    for change in changes {
        args.push("--edit".into());
        args.push(match &change.track_uid {
            Some(uid) => {
                uid.parse::<u64>().context("Invalid track UID")?;
                format!("track:={uid}").into()
            }
            None => "info".into(),
        });
        if change.property == Property::Language {
            for (key, field) in [("language", "legacy"), ("language-ietf", "ietf")] {
                let value = &change.after[field];
                if let Some(text) = value.as_str() {
                    ensure!(
                        text.len() <= 128 && !text.contains('\0'),
                        "Invalid language restoration value"
                    );
                    args.extend(["--set".into(), format!("{key}={text}").into()]);
                } else {
                    ensure!(value.is_null(), "Invalid language value");
                    args.extend(["--delete".into(), key.into()]);
                }
            }
        } else {
            let name = change.property.tool_name(change.track_uid.is_none());
            if change.after.is_null() {
                args.extend(["--delete".into(), name.into()]);
            } else {
                let text = if change.property == Property::Title {
                    let text = change.after.as_str().context("Title must be text")?;
                    ensure!(text.len() <= 4096 && !text.contains('\0'), "Invalid title");
                    text.to_string()
                } else {
                    if change.after.as_bool().context("Flag must be boolean")? {
                        "1"
                    } else {
                        "0"
                    }
                    .into()
                };
                args.extend(["--set".into(), format!("{name}={text}").into()]);
            }
        }
    }
    let command_length: usize = args
        .iter()
        .map(|arg| arg.to_string_lossy().encode_utf16().count() * 2 + 3)
        .sum();
    ensure!(
        command_length <= 28_000,
        "Changes exceed the Windows command-line limit; save a smaller group of properties"
    );
    Ok(args)
}

// Matroska utility/property names stay inside this adapter.
impl Property {
    pub fn tool_name(self, container: bool) -> &'static str {
        match self {
            Self::Title if container => "title",
            Self::Title => "name",
            Self::Language => "language",
            Self::Default => "flag-default",
            Self::Forced => "flag-forced",
            Self::Enabled => "flag-enabled",
            Self::HearingImpaired => "flag-hearing-impaired",
            Self::Commentary => "flag-commentary",
            Self::Original => "flag-original",
            Self::VisualImpaired => "flag-visual-impaired",
            Self::TextDescriptions => "flag-text-descriptions",
        }
    }
    pub fn json_key(self) -> &'static str {
        match self {
            Self::Title => "track_name",
            Self::Language => "language_ietf",
            Self::Default => "default_track",
            Self::Forced => "forced_track",
            Self::Enabled => "enabled_track",
            Self::HearingImpaired => "flag_hearing_impaired",
            Self::Commentary => "flag_commentary",
            Self::Original => "flag_original",
            Self::VisualImpaired => "flag_visual_impaired",
            Self::TextDescriptions => "flag_text_descriptions",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn audio_sample_fixture() -> (Value, Value, Value) {
        let source = json!({"tracks": [
            {"type":"video", "properties":{"uid":7, "number":1}},
            {"type":"audio", "properties":{
                "uid":608678619937247575_u64, "number":2
            }}
        ]});
        let sample = json!({"tracks": [{"type":"audio", "properties":{
            "uid":608678619937247575_u64, "number":1
        }}]});
        let media = json!({"media":{"track":[
            {"@type":"General", "Duration":"30"},
            {"@type":"Audio", "UniqueID":"608678619937247575",
             "ID":"1", "ChannelLayout":"L R C LFE Ls Rs",
             "Format_AdditionalFeatures":"JOC", "Duration":"30",
             "BitRate":"768000", "StreamSize":"2880000"}
        ]}});
        (source, sample, media)
    }

    #[test]
    fn audio_sample_remaps_verified_identity_and_excludes_sample_statistics() {
        let (source, sample, media) = audio_sample_fixture();
        let result = audio_sample_info(&source, &sample, &media).unwrap();
        let tracks = result["media"]["track"].as_array().unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0]["ID"], "2");
        assert_eq!(tracks[0]["ChannelLayout"], "L R C LFE Ls Rs");
        assert_eq!(tracks[0]["Format_AdditionalFeatures"], "JOC");
        for key in ["Duration", "BitRate", "StreamSize"] {
            assert!(tracks[0].get(key).is_none());
        }
        assert_eq!(result["tracksmith_analysis"]["scope"], "audio_sample");
    }

    #[test]
    fn audio_sample_rejects_unverified_and_ambiguous_identities() {
        let (source, sample, media) = audio_sample_fixture();
        let mut wrong = media.clone();
        wrong["media"]["track"][1]["ID"] = json!(2);
        assert!(audio_sample_info(&source, &sample, &wrong).is_err());
        wrong["media"]["track"][1]["ID"] = json!(1);
        wrong["media"]["track"][1]["UniqueID"] = json!(99);
        assert!(audio_sample_info(&source, &sample, &wrong).is_err());
        let mut duplicate = sample.clone();
        duplicate["tracks"]
            .as_array_mut()
            .unwrap()
            .push(sample["tracks"][0].clone());
        assert!(audio_sample_info(&source, &duplicate, &media).is_err());
        let mut duplicate = source.clone();
        duplicate["tracks"]
            .as_array_mut()
            .unwrap()
            .push(source["tracks"][1].clone());
        assert!(audio_sample_info(&duplicate, &sample, &media).is_err());
        let mut duplicate = media.clone();
        duplicate["media"]["track"]
            .as_array_mut()
            .unwrap()
            .push(media["media"]["track"][1].clone());
        assert!(audio_sample_info(&source, &sample, &duplicate).is_err());
    }

    #[test]
    fn commands_use_uid_and_keep_untrusted_titles_in_one_argument() {
        let change = Change {
            track_uid: Some("18446744073709551615".into()),
            property: Property::Title,
            before: Value::Null,
            after: json!("name & --delete tracks"),
        };
        let args = command_args(Path::new("movie.mkv"), &[change]).unwrap();
        assert!(args.contains(&OsString::from("track:=18446744073709551615")));
        assert_eq!(args.last().unwrap(), "name=name & --delete tracks");
        assert_eq!(
            normalize_language("en-GB").unwrap(),
            json!({"legacy":"eng","ietf":"en-GB"})
        );
        assert_eq!(normalize_language("fr").unwrap()["legacy"], "fre");
        assert_eq!(normalize_language("fil").unwrap()["legacy"], "fil");
        assert_eq!(normalize_language("cmn").unwrap()["legacy"], "und");
        assert!(normalize_language("invalid --set").is_err());
    }
}
