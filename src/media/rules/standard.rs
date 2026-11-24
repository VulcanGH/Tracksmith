//! Factory defaults are ordinary, editable rules. Existing saved libraries are never replaced.
use super::model::*;
use crate::media::model::Property;
use serde_json::json;

pub const AUDIO: &[&str] = &[
    "Dolby Digital",
    "Dolby Digital Plus",
    "Dolby TrueHD",
    "DTS",
    "DTS-ES",
    "DTS-ES Discrete",
    "DTS-ES Matrix",
    "DTS 96/24",
    "DTS-HD",
    "DTS-HD MA",
    "DTS-HD HRA",
    "DTS Express",
    "AAC",
    "AAC-LC",
    "HE-AAC",
    "HE-AAC v2",
    "FLAC",
    "ALAC",
    "LPCM",
    "Opus",
    "Vorbis",
    "MP3",
    "MP2",
    "MP1",
];
pub const SUBTITLES: &[&str] = &["SRT", "PGS", "ASS", "SSA", "VobSub", "WebVTT", "DVB"];
pub fn library() -> RuleLibrary {
    let mut rules = vec![];
    for (scope, labels) in [(Scope::Audio, AUDIO), (Scope::Subtitles, SUBTITLES)] {
        for label in labels {
            let audio = scope == Scope::Audio;
            let display = if *label == "DTS-HD MA" {
                "DTS-HD Master Audio"
            } else {
                label
            };
            let id = format!(
                "standard-{}-{}",
                if audio { "audio" } else { "sub" },
                label.to_lowercase().replace([' ', '/'], "-")
            );
            let template = if audio {
                format!(
                    "{{language_display}} {display}{{channel_suffix}}{{immersive_suffix}}{{audio_role_suffix}}"
                )
            } else {
                format!("{{language_display}} ({label}{{subtitle_qualifiers}})")
            };
            rules.push(Rule {
                id, name: format!("{}: {display}", if audio { "Audio" } else { "Subtitles" }),
                description: if audio { "Standard naming: language, confirmed codec/profile, reported channel layout (count fallback), confirmed Atmos/DTS:X and known roles. Titles only. Edit this template to set your convention." } else { "Standard naming: language, subtitle format, then explicit Forced, SDH/CC, Commentary and Signs & Songs qualifiers. Format is never used to infer subtitle purpose. Titles only." }.into(),
                enabled: true, priority: rules.len() as u32, scope,
                conditions: ConditionGroup { mode: MatchMode::All, conditions: vec![Condition { field: Field::NamingCodec, operator: Operator::Equals, value: json!(label), case_sensitive: true }], groups: vec![] },
                actions: vec![Action { property: Property::Title, source: ValueSource::Template, value: json!(template), missing: MissingValue::SkipTarget }],
                overwrite: OverwritePolicy::Always, replace_when: None,
            });
        }
    }
    rules.push(Rule {
        id: "standard-movie-title".into(), name: "Video track title: filename without extension".into(),
        description: "Set each video track title to the exact filename without its final extension. Keep punctuation, spacing and release details. Leaves the container title and audio/subtitle naming unchanged.".into(),
        enabled: true, priority: rules.len() as u32, scope: Scope::Video,
        conditions: ConditionGroup { mode: MatchMode::All, conditions: vec![], groups: vec![] },
        actions: vec![Action { property: Property::Title, source: ValueSource::Template, value: json!("{filename_stem}"), missing: MissingValue::SkipTarget }],
        overwrite: OverwritePolicy::Always, replace_when: None,
    });
    RuleLibrary { version: 1, revision: 0, active_profile_id: "general".into(), profiles: vec![Profile {
        id: "general".into(), name: "Tracksmith standard".into(),
        description: "Editable standard audio/subtitle titles and video track title from filename. Automatically stage on open; always review before saving. Leaves the container title, language codes, flags and track order unchanged. Unknown audio/subtitle formats retain their titles.".into(),
        rule_ids: rules.iter().map(|r|r.id.clone()).collect(), preferred_languages: vec![], auto_apply: true, overwrite_policy: None,
    }], rules }
}
