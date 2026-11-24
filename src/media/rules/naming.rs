//! Display facts from identified codecs and validated per-track enrichment. Never infer codecs
//! or immersive formats from a track title. Role labels may retain explicit existing descriptions.
use super::facts::ContextData;
use crate::media::model::{Property, Track};
use std::collections::BTreeSet;

fn info<'a>(track: &'a Track, key: &str) -> &'a str {
    track
        .mediainfo
        .as_ref()
        .and_then(|m| m.get(key))
        .and_then(|v| v.as_str())
        .unwrap_or("")
}
fn id(track: &Track) -> &str {
    track
        .technical
        .get("codec_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
}
fn format_evidence(track: &Track) -> String {
    [
        track.codec.as_str(),
        info(track, "Format"),
        info(track, "Format_Profile"),
        info(track, "Format_AdditionalFeatures"),
        info(track, "Format_Commercial_IfAny"),
        info(track, "Format_Commercial"),
    ]
    .join(" ")
    .to_ascii_uppercase()
}
pub fn codec(track: &Track) -> Option<&'static str> {
    let code = id(track).to_ascii_uppercase();
    let evidence = format_evidence(track);
    let format = info(track, "Format").to_ascii_uppercase();
    let raw = track.codec.to_ascii_uppercase();
    if track.kind == "subtitles" {
        return Some(match code.as_str() {
            "S_TEXT/UTF8" => "SRT",
            "S_HDMV/PGS" => "PGS",
            "S_TEXT/ASS" => "ASS",
            "S_TEXT/SSA" => "SSA",
            "S_VOBSUB" => "VobSub",
            "S_TEXT/WEBVTT" => "WebVTT",
            "S_DVBSUB" => "DVB",
            _ => match raw.as_str() {
                "SUBRIP/SRT" | "SUBRIP" | "SRT" => "SRT",
                "HDMV PGS" | "PGS" => "PGS",
                "SUBSTATIONALPHA" | "ASS" if format == "ASS" || raw == "ASS" => "ASS",
                "SSA" => "SSA",
                "VOBSUB" => "VobSub",
                "WEBVTT" => "WebVTT",
                _ => return None,
            },
        });
    }
    if track.kind != "audio" {
        return None;
    }
    if code == "A_TRUEHD" || evidence.contains("TRUEHD") || format.starts_with("MLP FBA") {
        return Some("Dolby TrueHD");
    }
    if code == "A_EAC3"
        || matches!(format.as_str(), "E-AC-3" | "E-AC-3 JOC")
        || evidence.contains("DOLBY DIGITAL PLUS")
    {
        return Some("Dolby Digital Plus");
    }
    if code == "A_AC3" || format == "AC-3" || raw == "AC-3" || raw == "DOLBY DIGITAL" {
        return Some("Dolby Digital");
    }
    if code == "A_DTS" || format == "DTS" || raw.starts_with("DTS") {
        let features = info(track, "Format_AdditionalFeatures").to_ascii_uppercase();
        return Some(
            if evidence.contains("MASTER AUDIO")
                || evidence.contains("DTS-HD MA")
                || evidence.contains("XLL")
            {
                "DTS-HD MA"
            } else if evidence.contains("HIGH RESOLUTION")
                || evidence.contains("DTS-HD HRA")
                || features.split_whitespace().any(|v| v == "XBR")
            {
                "DTS-HD HRA"
            } else if evidence.contains("EXPRESS")
                || features.split_whitespace().any(|v| v == "LBR")
            {
                "DTS Express"
            } else if evidence.contains("DTS-HD") {
                "DTS-HD"
            } else if evidence.contains("DISCRETE")
                && (evidence.contains("DTS-ES") || features.split_whitespace().any(|v| v == "ES"))
            {
                "DTS-ES Discrete"
            } else if evidence.contains("MATRIX")
                && (evidence.contains("DTS-ES") || features.split_whitespace().any(|v| v == "ES"))
            {
                "DTS-ES Matrix"
            } else if evidence.contains("DTS-ES") || features.split_whitespace().any(|v| v == "ES")
            {
                "DTS-ES"
            } else if evidence.contains("96/24") {
                "DTS 96/24"
            } else {
                "DTS"
            },
        );
    }
    if code.starts_with("A_AAC")
        || format == "AAC"
        || raw.starts_with("AAC")
        || format.starts_with("HE-AAC")
    {
        let profile = format!(
            "{} {} {}",
            info(track, "Format_Profile"),
            info(track, "Format_AdditionalFeatures"),
            code
        )
        .to_ascii_uppercase();
        let sbr = profile.contains("SBR")
            || info(track, "Format_Settings_SBR")
                .to_ascii_lowercase()
                .starts_with("yes");
        let ps = profile
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|v| v == "PS")
            || info(track, "Format_Settings_PS")
                .to_ascii_lowercase()
                .starts_with("yes");
        return Some(
            if evidence.contains("HE-AACV2") || evidence.contains("HE-AAC V2") || (sbr && ps) {
                "HE-AAC v2"
            } else if evidence.contains("HE-AAC") || sbr {
                "HE-AAC"
            } else if profile
                .split(|c: char| !c.is_ascii_alphanumeric())
                .any(|v| v == "LC")
            {
                "AAC-LC"
            } else {
                "AAC"
            },
        );
    }
    Some(match code.as_str() {
        "A_FLAC" => "FLAC",
        "A_ALAC" => "ALAC",
        "A_OPUS" => "Opus",
        "A_VORBIS" => "Vorbis",
        "A_MPEG/L3" => "MP3",
        "A_MPEG/L2" => "MP2",
        "A_MPEG/L1" => "MP1",
        v if v.starts_with("A_PCM/INT/") || v == "A_PCM/FLOAT/IEEE" => "LPCM",
        _ => match raw.as_str() {
            "FLAC" => "FLAC",
            "ALAC" => "ALAC",
            "OPUS" => "Opus",
            "VORBIS" => "Vorbis",
            "MP3" | "MPEG-1 LAYER 3" | "MPEG-1 LAYER III" => "MP3",
            "MP2" | "MPEG-1 LAYER 2" | "MPEG-1 LAYER II" => "MP2",
            "PCM" | "LPCM" => "LPCM",
            _ => return None,
        },
    })
}
fn count(track: &Track) -> Option<usize> {
    info(track, "Channels")
        .parse()
        .ok()
        .or_else(|| {
            track
                .technical
                .get("audio_channels")
                .and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
        })
        .and_then(|n| usize::try_from(n).ok())
        .filter(|n| *n > 0 && *n <= 256)
}
pub fn channels(track: &Track) -> String {
    let count = count(track);
    let layout = info(track, "ChannelLayout");
    let parts: Vec<_> = layout.split_whitespace().collect();
    let unique: BTreeSet<_> = parts.iter().copied().collect();
    // Accept one explicit layout only. Duplicate, conflicting or unknown tokens fall back to count.
    let recognized = [
        "M", "L", "R", "C", "LFE", "LFE2", "Ls", "Rs", "Lb", "Rb", "Lss", "Rss", "Lc", "Rc", "Cs",
        "Cb", "Lw", "Rw", "Tfl", "Tfr", "Tfc", "Tsl", "Tsr", "Tbl", "Tbr", "Tbc", "Tc",
    ];
    if !parts.is_empty()
        && unique.len() == parts.len()
        && count.is_none_or(|n| n == parts.len())
        && parts.iter().all(|p| recognized.contains(p))
    {
        let lfe = parts
            .iter()
            .filter(|p| matches!(**p, "LFE" | "LFE2"))
            .count();
        let height = parts.iter().filter(|p| p.starts_with('T')).count();
        let mains = parts.len() - lfe - height;
        return if height > 0 {
            format!(" {mains}.{lfe}.{height}")
        } else {
            format!(" {mains}.{lfe}")
        };
    }
    count.map(|n| format!(" {n}ch")).unwrap_or_default()
}
pub fn immersive(track: &Track) -> &'static str {
    let evidence = format_evidence(track);
    match codec(track) {
        Some("Dolby TrueHD" | "Dolby Digital Plus")
            if evidence.contains("ATMOS")
                || (codec(track) == Some("Dolby Digital Plus")
                    && info(track, "Format_AdditionalFeatures")
                        .split_whitespace()
                        .any(|v| v.eq_ignore_ascii_case("JOC"))) =>
        {
            " + Atmos"
        }
        Some("DTS-HD MA")
            if evidence.contains("DTS:X")
                || info(track, "Format_AdditionalFeatures")
                    .split_whitespace()
                    .any(|v| v == "X") =>
        {
            " + DTS:X"
        }
        _ => "",
    }
}
fn marked(title: &str, word: &str) -> bool {
    title
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|v| v.eq_ignore_ascii_case(word))
}
fn commentary_detail(title: &str) -> Option<String> {
    let value = title.trim();
    let lower = value.to_ascii_lowercase();
    for marker in ["commentary — ", "commentary - ", "commentary by "] {
        if let Some(start) = lower.rfind(marker) {
            return Some(
                value[start + marker.len()..]
                    .trim_end_matches(')')
                    .split(", ")
                    .next()
                    .unwrap_or("")
                    .trim()
                    .into(),
            );
        }
    }
    let segment = value.rsplit('(').next()?.trim_end_matches(')').trim();
    if segment.to_lowercase().ends_with(" commentary") {
        return Some(segment[..segment.len() - " commentary".len()].trim().into());
    }
    None
}
fn flag(context: &ContextData<'_>, property: Property) -> bool {
    context
        .property(property)
        .value
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}
pub fn audio_role(context: &ContextData<'_>) -> String {
    let title = context
        .track()
        .and_then(|t| t.title.as_deref())
        .unwrap_or("");
    let mut roles = vec![];
    if flag(context, Property::Commentary) || marked(title, "commentary") {
        roles.push(
            commentary_detail(title)
                .filter(|s| !s.is_empty())
                .map(|s| format!("Commentary — {s}"))
                .unwrap_or_else(|| "Commentary".into()),
        );
    }
    let lower = title.to_ascii_lowercase();
    if flag(context, Property::VisualImpaired)
        || lower.contains("audio description")
        || lower.contains("audio descriptive")
    {
        roles.push("Audio Description".into());
    }
    if roles.is_empty() {
        String::new()
    } else {
        format!(" ({})", roles.join(", "))
    }
}
pub fn subtitle_qualifiers(context: &ContextData<'_>) -> String {
    let title = context
        .track()
        .and_then(|t| t.title.as_deref())
        .unwrap_or("");
    let lower = title.to_ascii_lowercase();
    let mut labels = vec![];
    if flag(context, Property::Forced) || marked(title, "forced") {
        labels.push("Forced".to_owned());
    }
    if flag(context, Property::HearingImpaired)
        || marked(title, "sdh")
        || lower.contains("hearing impaired")
        || lower.contains("hearing-impaired")
    {
        labels.push("SDH".into());
    }
    if marked(title, "cc") {
        labels.push("CC".into());
    }
    if flag(context, Property::Commentary) || marked(title, "commentary") {
        labels.push(
            commentary_detail(title)
                .map(|s| format!("Commentary — {s}"))
                .unwrap_or_else(|| "Commentary".into()),
        );
    }
    if lower.contains("signs & songs") || lower.contains("signs and songs") {
        labels.push("Signs & Songs".into());
    }
    if labels.is_empty() {
        String::new()
    } else {
        format!(", {}", labels.join(", "))
    }
}
