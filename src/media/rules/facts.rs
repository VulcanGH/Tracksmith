use super::model::*;
use crate::media::model::{Inspection, Property, Track};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path, sync::LazyLock};

pub type Overlay = BTreeMap<EditKey, Value>;
pub struct Fact {
    pub value: Option<Value>,
    pub undefined: bool,
}
impl Fact {
    fn known(value: Option<Value>) -> Self {
        Self {
            undefined: value.is_none(),
            value,
        }
    }
    pub fn empty(&self) -> bool {
        self.value
            .as_ref()
            .is_none_or(|v| v.is_null() || v.as_str() == Some(""))
    }
}
pub struct ContextData<'a> {
    pub doc: &'a Inspection,
    pub profile: &'a Profile,
    pub overlay: &'a Overlay,
    pub target: Option<usize>,
}

pub fn canonical_language(value: &str) -> Result<String> {
    let (primary, suffix) = value
        .split_once('-')
        .map(|(a, b)| (a, format!("-{b}")))
        .unwrap_or((value, String::new()));
    let lower = primary.to_ascii_lowercase();
    static ALIASES: LazyLock<BTreeMap<String, String>> = LazyLock::new(|| {
        let mut map = BTreeMap::new();
        for line in include_str!("../../../data/iso639-2.txt").lines() {
            let fields: Vec<_> = line.split('|').collect();
            let preferred = if !fields[2].is_empty() {
                fields[2]
            } else if !fields[1].is_empty() {
                fields[1]
            } else {
                fields[0]
            };
            for code in fields {
                if !code.is_empty() {
                    map.insert(code.into(), preferred.into());
                }
            }
        }
        map
    });
    let normalized = ALIASES.get(&lower).map(String::as_str).unwrap_or(primary);
    let parsed = language_tags::LanguageTag::parse(&format!("{normalized}{suffix}"))
        .context("Invalid BCP 47 language")?;
    parsed.validate().context("Unregistered BCP 47 language")?;
    Ok(parsed.canonicalize()?.to_string())
}
fn language_name(tag: &str) -> Option<String> {
    static NAMES: LazyLock<BTreeMap<String, String>> = LazyLock::new(|| {
        serde_json::from_str(include_str!("../../../data/language-names.json"))
            .expect("Embedded language names are valid JSON")
    });
    if tag == "und" {
        return None;
    }
    NAMES.get(tag.split('-').next()?).cloned()
}
fn display_language(tag: Option<&str>) -> String {
    let Some(tag) = tag
        .and_then(|t| canonical_language(t).ok())
        .filter(|t| t != "und")
    else {
        return "Unknown".into();
    };
    let name = language_name(&tag).unwrap_or_else(|| tag.clone());
    let Some((_, suffix)) = tag.split_once('-') else {
        return name;
    };
    let qualifiers: Vec<_> = suffix
        .split('-')
        .map(|part| match part {
            "BR" => "Brazil",
            "PT" => "Portugal",
            "US" => "United States",
            "GB" => "United Kingdom",
            "CA" => "Canada",
            "AU" => "Australia",
            "NZ" => "New Zealand",
            "MX" => "Mexico",
            "ES" => "Spain",
            "419" => "Latin America",
            "CN" => "China",
            "TW" => "Taiwan",
            "HK" => "Hong Kong",
            "Hans" => "Simplified",
            "Hant" => "Traditional",
            "Latn" => "Latin",
            "Cyrl" => "Cyrillic",
            other => other,
        })
        .collect();
    format!("{name} ({})", qualifiers.join(", "))
}
fn mi(track: Option<&Track>, key: &str) -> Option<Value> {
    track
        .and_then(|t| t.mediainfo.as_ref())
        .and_then(|m| m.get(key))
        .filter(|v| !v.is_null() && v.as_str() != Some(""))
        .cloned()
}
fn technical(track: Option<&Track>, key: &str) -> Option<Value> {
    track
        .and_then(|t| t.technical.get(key))
        .filter(|v| !v.is_null())
        .cloned()
}
fn numeric(value: Option<Value>) -> Option<Value> {
    value
        .and_then(|v| {
            v.as_f64()
                .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
        })
        .filter(|v| v.is_finite())
        .map(|n| json!(n))
}
impl ContextData<'_> {
    pub fn key(&self, property: Property) -> EditKey {
        EditKey {
            track_uid: self.target.map(|index| self.doc.tracks[index].uid.clone()),
            property,
        }
    }
    pub fn track(&self) -> Option<&Track> {
        self.target.map(|index| &self.doc.tracks[index])
    }
    pub fn property(&self, property: Property) -> Fact {
        let key = self.key(property);
        let value = self
            .overlay
            .get(&key)
            .cloned()
            .or_else(|| {
                let track = self.track();
                match property {
                    Property::Title => track
                        .and_then(|t| t.title.clone())
                        .or_else(|| {
                            if track.is_none() {
                                self.doc.title.clone()
                            } else {
                                None
                            }
                        })
                        .map(Value::String),
                    Property::Language => track
                        .and_then(|t| t.language_ietf.clone().or(t.language.clone()))
                        .map(Value::String),
                    other => track
                        .and_then(|t| {
                            t.flags
                                .get(serde_json::to_value(other).ok()?.as_str()?)
                                .copied()
                                .flatten()
                        })
                        .map(Value::Bool),
                }
            })
            .filter(|v| !v.is_null());
        if property == Property::Language {
            let value = value
                .and_then(|v| {
                    v.as_str()
                        .map(|s| canonical_language(s).unwrap_or_else(|_| s.into()))
                })
                .map(Value::String);
            let undefined = value.as_ref().is_none_or(|v| v.as_str() == Some("und"));
            Fact { value, undefined }
        } else {
            Fact::known(value)
        }
    }
    pub fn field(&self, field: Field) -> Fact {
        let track = self.track();
        let value = match field {
            Field::ExistingTitle => return self.property(Property::Title),
            Field::Language => return self.property(Property::Language),
            Field::LanguageName => self
                .property(Property::Language)
                .value
                .and_then(|v| language_name(v.as_str()?))
                .map(Value::String),
            Field::Default => return self.property(Property::Default),
            Field::Forced => return self.property(Property::Forced),
            Field::Enabled => return self.property(Property::Enabled),
            Field::HearingImpaired => return self.property(Property::HearingImpaired),
            Field::Commentary => return self.property(Property::Commentary),
            Field::Original => return self.property(Property::Original),
            Field::VisualImpaired => return self.property(Property::VisualImpaired),
            Field::TextDescriptions => return self.property(Property::TextDescriptions),
            Field::Container => Some(json!(self.doc.container)),
            Field::HasVideo => Some(json!(self.doc.tracks.iter().any(|t| t.kind == "video"))),
            Field::Filename => Path::new(&self.doc.path)
                .file_name()
                .map(|name| json!(name.to_string_lossy())),
            Field::FilePath => Some(json!(
                if let Some(unc) = self.doc.path.strip_prefix(r"\\?\UNC\") {
                    format!(r"\\{unc}")
                } else {
                    self.doc
                        .path
                        .strip_prefix(r"\\?\")
                        .unwrap_or(&self.doc.path)
                        .to_string()
                }
            )),
            Field::TrackType => track.map(|t| json!(t.kind)),
            Field::Codec => track.map(|t| json!(t.codec)),
            Field::NamingCodec => track.and_then(super::naming::codec).map(|v| json!(v)),
            Field::CodecProfile => mi(track, "Format_Profile"),
            Field::ExtendedFormat => {
                let values: Vec<_> = [
                    "Format_AdditionalFeatures",
                    "Format_Commercial_IfAny",
                    "Format_Commercial",
                ]
                .iter()
                .filter_map(|key| mi(track, key).and_then(|v| v.as_str().map(str::to_owned)))
                .collect();
                (!values.is_empty()).then(|| json!(values.join(" · ")))
            }
            Field::ChannelCount => {
                numeric(technical(track, "audio_channels").or_else(|| mi(track, "Channels")))
            }
            Field::ChannelLayout => mi(track, "ChannelLayout"),
            Field::Bitrate => numeric(mi(track, "BitRate")),
            Field::SampleRate => numeric(
                technical(track, "audio_sampling_frequency").or_else(|| mi(track, "SamplingRate")),
            ),
            Field::BitDepth => {
                numeric(technical(track, "audio_bits_per_sample").or_else(|| mi(track, "BitDepth")))
            }
            Field::Width => numeric(mi(track, "Width").or_else(|| {
                technical(track, "pixel_dimensions")
                    .and_then(|v| v.as_str()?.split_once('x').map(|(w, _)| json!(w)))
            })),
            Field::Height => numeric(mi(track, "Height").or_else(|| {
                technical(track, "pixel_dimensions")
                    .and_then(|v| v.as_str()?.split_once('x').map(|(_, h)| json!(h)))
            })),
            Field::Resolution => technical(track, "pixel_dimensions"),
            Field::HdrFormat => mi(track, "HDR_Format"),
            Field::DolbyVisionProfile => mi(track, "HDR_Format").and_then(|v| {
                v.as_str()
                    .is_some_and(|s| s.to_lowercase().contains("dolby vision"))
                    .then(|| mi(track, "HDR_Format_Profile"))
                    .flatten()
            }),
            Field::TrackPosition => self.target.map(|index| json!(index + 1)),
            Field::Lossless => mi(track, "Compression_Mode").and_then(|v| {
                match v.as_str()?.to_lowercase().as_str() {
                    "lossless" => Some(json!(true)),
                    "lossy" => Some(json!(false)),
                    _ => None,
                }
            }),
        };
        Fact::known(value)
    }
    pub fn variable(&self, name: &str) -> Option<String> {
        match name {
            "language_display" => {
                return Some(display_language(
                    self.property(Property::Language)
                        .value
                        .as_ref()
                        .and_then(Value::as_str),
                ));
            }
            "naming_codec" => {
                return self
                    .track()
                    .and_then(super::naming::codec)
                    .map(str::to_owned);
            }
            "channel_suffix" => return self.track().map(super::naming::channels),
            "immersive_suffix" => return self.track().map(|t| super::naming::immersive(t).into()),
            "audio_role_suffix" => return Some(super::naming::audio_role(self)),
            "subtitle_qualifiers" => return Some(super::naming::subtitle_qualifiers(self)),
            _ => {}
        }
        let field = match name {
            "codec" => Field::Codec,
            "codec_profile" => Field::CodecProfile,
            "extended_format" => Field::ExtendedFormat,
            "channels" | "channel_count" => Field::ChannelCount,
            "channel_layout" => Field::ChannelLayout,
            "bitrate" => Field::Bitrate,
            "sample_rate" => Field::SampleRate,
            "bit_depth" => Field::BitDepth,
            "resolution" => Field::Resolution,
            "width" => Field::Width,
            "height" => Field::Height,
            "hdr_format" => Field::HdrFormat,
            "dolby_vision_profile" => Field::DolbyVisionProfile,
            "language" => Field::Language,
            "language_name" => Field::LanguageName,
            "track_position" => Field::TrackPosition,
            "filename" => Field::Filename,
            "container" => Field::Container,
            "filename_stem" => {
                return Path::new(&self.doc.path)
                    .file_stem()
                    .map(|v| v.to_string_lossy().into_owned());
            }
            "preferred_language" => {
                return self
                    .profile
                    .preferred_languages
                    .first()
                    .and_then(|v| canonical_language(v).ok());
            }
            "preferred_language_name" => {
                return self
                    .profile
                    .preferred_languages
                    .first()
                    .and_then(|v| canonical_language(v).ok())
                    .and_then(|v| language_name(&v));
            }
            _ => return None,
        };
        let fact = self.field(field);
        if fact.undefined {
            return None;
        }
        fact.value.map(|v| match v {
            Value::String(s) => s,
            Value::Number(n) if n.as_f64().is_some_and(|v| v.fract() == 0.0) => {
                format!("{:.0}", n.as_f64().unwrap())
            }
            other => other.to_string(),
        })
    }
}
