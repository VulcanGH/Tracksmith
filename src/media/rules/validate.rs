use super::{facts::canonical_language, model::*};
use crate::media::model::Property;
use anyhow::{Context, Result, bail, ensure};
use regex::{Regex, RegexBuilder};
use std::collections::HashSet;

pub fn field_kind(field: Field) -> &'static str {
    match field {
        Field::ChannelCount
        | Field::Bitrate
        | Field::SampleRate
        | Field::BitDepth
        | Field::Width
        | Field::Height
        | Field::TrackPosition => "number",
        Field::HasVideo
        | Field::Default
        | Field::Forced
        | Field::Enabled
        | Field::HearingImpaired
        | Field::Commentary
        | Field::Original
        | Field::VisualImpaired
        | Field::TextDescriptions
        | Field::Lossless => "boolean",
        _ => "text",
    }
}
pub fn operators(field: Field) -> Vec<Operator> {
    use Operator::*;
    match field_kind(field) {
        "number" => vec![Equals, DoesNotEqual, GreaterThan, LessThan, IsUndefined],
        "boolean" => vec![Equals, DoesNotEqual, IsUndefined],
        _ => vec![
            Equals,
            DoesNotEqual,
            Contains,
            DoesNotContain,
            StartsWith,
            EndsWith,
            Regex,
            IsEmpty,
            IsNotEmpty,
            IsUndefined,
        ],
    }
}
pub fn bounded_regex(condition: &Condition) -> Result<Regex> {
    RegexBuilder::new(condition.value.as_str().context("Regex must be text")?)
        .case_insensitive(!condition.case_sensitive)
        .size_limit(1024 * 1024)
        .dfa_size_limit(1024 * 1024)
        .nest_limit(64)
        .build()
        .context("Invalid or overly complex regular expression")
}
pub fn template_parts(template: &str) -> Result<Vec<(bool, String)>> {
    ensure!(
        template.len() <= 4096 && !template.contains('\0'),
        "Template exceeds 4096 bytes or contains a NUL"
    );
    let names: HashSet<_> = catalog().variables.into_iter().map(|v| v.name).collect();
    let mut parts = vec![];
    let mut literal = String::new();
    let mut chars = template.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                literal.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                literal.push('}');
            }
            '{' => {
                if !literal.is_empty() {
                    parts.push((false, std::mem::take(&mut literal)));
                }
                let mut name = String::new();
                let mut closed = false;
                for ch in chars.by_ref() {
                    if ch == '}' {
                        closed = true;
                        break;
                    }
                    ensure!(
                        ch.is_ascii_lowercase() || ch == '_',
                        "Invalid template variable"
                    );
                    name.push(ch);
                }
                ensure!(
                    closed && names.contains(&name),
                    "Unknown or unclosed template variable {{{name}}}"
                );
                parts.push((true, name));
            }
            '}' => bail!("Unmatched closing brace in template; use }}}} for a literal brace"),
            other => literal.push(other),
        }
    }
    if !literal.is_empty() {
        parts.push((false, literal));
    }
    Ok(parts)
}
fn id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn group(group: &ConditionGroup, depth: usize, count: &mut usize) -> Result<()> {
    ensure!(depth <= 4, "Condition groups may nest at most four levels");
    *count += 1 + group.conditions.len();
    ensure!(
        *count <= 64,
        "A rule may contain at most 64 condition/group nodes"
    );
    for condition in &group.conditions {
        ensure!(
            operators(condition.field).contains(&condition.operator),
            "Operator is not valid for this field"
        );
        if matches!(
            condition.operator,
            Operator::IsEmpty | Operator::IsNotEmpty | Operator::IsUndefined
        ) {
            ensure!(condition.value.is_null(), "This operator takes no value");
            continue;
        }
        match field_kind(condition.field) {
            "number" => ensure!(
                condition.value.as_f64().is_some_and(|v| v.is_finite()),
                "Numeric condition needs a finite number"
            ),
            "boolean" => ensure!(
                condition.value.is_boolean(),
                "Boolean condition needs true or false"
            ),
            _ => ensure!(
                condition
                    .value
                    .as_str()
                    .is_some_and(|v| v.len() <= 1024 && !v.contains('\0')),
                "Text conditions are limited to 1024 bytes"
            ),
        }
        if condition.operator == Operator::Regex {
            bounded_regex(condition)?;
        }
    }
    for nested in &group.groups {
        self::group(nested, depth + 1, count)?;
    }
    Ok(())
}
pub fn validate_rule(rule: &Rule) -> Result<()> {
    ensure!(
        id(&rule.id)
            && !rule.name.trim().is_empty()
            && rule.name.len() <= 128
            && rule.description.len() <= 2048,
        "Rule ID, name or description is invalid"
    );
    let mut count = 0;
    group(&rule.conditions, 0, &mut count)?;
    if rule.overwrite == OverwritePolicy::Conditional {
        let replacement = rule
            .replace_when
            .as_ref()
            .context("Conditional replacement requires conditions")?;
        ensure!(
            !replacement.conditions.is_empty() || !replacement.groups.is_empty(),
            "Conditional replacement needs at least one condition"
        );
    }
    if let Some(replacement) = &rule.replace_when {
        group(replacement, 0, &mut count)?;
    }
    ensure!(
        !rule.actions.is_empty() && rule.actions.len() <= 10,
        "A rule needs between one and ten actions"
    );
    let mut properties = HashSet::new();
    for action in &rule.actions {
        ensure!(
            properties.insert(action.property),
            "A rule cannot set the same property twice"
        );
        ensure!(
            rule.scope != Scope::Container || action.property == Property::Title,
            "Only the container title is editable"
        );
        match action.source {
            ValueSource::Clear => ensure!(
                action.property == Property::Title && action.value.is_null(),
                "Only titles can be cleared"
            ),
            ValueSource::Template => {
                ensure!(
                    action.property == Property::Title,
                    "Templates can only set titles"
                );
                let text = action.value.as_str().context("Template must be text")?;
                ensure!(!text.trim().is_empty(), "Template is empty");
                template_parts(text)?;
            }
            ValueSource::PreferredLanguage => ensure!(
                action.property == Property::Language && action.value.is_null(),
                "Preferred language actions must target language"
            ),
            ValueSource::Literal => match action.property {
                Property::Title => ensure!(
                    action
                        .value
                        .as_str()
                        .is_some_and(|v| !v.is_empty() && v.len() <= 4096 && !v.contains('\0')),
                    "Title must be nonempty text; use Clear title to remove it"
                ),
                Property::Language => {
                    canonical_language(action.value.as_str().context("Language must be text")?)?;
                }
                _ => ensure!(action.value.is_boolean(), "Flag action must be boolean"),
            },
        }
    }
    Ok(())
}
pub fn validate(library: &RuleLibrary) -> Result<()> {
    ensure!(library.version == 1, "Unsupported rules library version");
    ensure!(
        library.rules.len() <= 128 && !library.profiles.is_empty() && library.profiles.len() <= 32,
        "Use at most 128 rules and between one and 32 profiles"
    );
    let mut ids = HashSet::new();
    for rule in &library.rules {
        ensure!(ids.insert(rule.id.as_str()), "Duplicate rule ID");
        validate_rule(rule).with_context(|| format!("Rule {}", rule.name))?;
    }
    let mut profiles = HashSet::new();
    for profile in &library.profiles {
        ensure!(
            id(&profile.id) && profiles.insert(profile.id.as_str()),
            "Invalid or duplicate profile ID"
        );
        ensure!(
            !profile.name.trim().is_empty()
                && profile.name.len() <= 128
                && profile.description.len() <= 2048,
            "Invalid profile name or description"
        );
        ensure!(
            profile.rule_ids.len() <= 128 && profile.preferred_languages.len() <= 16,
            "Profile has too many rules or preferred languages"
        );
        let mut assigned = HashSet::new();
        for rule in &profile.rule_ids {
            ensure!(
                ids.contains(rule.as_str()) && assigned.insert(rule),
                "Unknown or duplicated profile rule assignment"
            );
        }
        for lang in &profile.preferred_languages {
            canonical_language(lang)?;
        }
        ensure!(
            profile.overwrite_policy != Some(OverwritePolicy::Conditional),
            "Conditional replacement is configured per rule"
        );
    }
    ensure!(
        profiles.contains(library.active_profile_id.as_str()),
        "Active profile does not exist"
    );
    Ok(())
}
pub fn catalog() -> RuleCatalog {
    use Field::*;
    let fields = [
        (Container, "Container format"),
        (HasVideo, "File contains video"),
        (Filename, "Filename"),
        (FilePath, "File path"),
        (TrackType, "Track type"),
        (Codec, "Codec"),
        (
            NamingCodec,
            "Standard codec/format name (identified evidence)",
        ),
        (CodecProfile, "Codec profile"),
        (ExtendedFormat, "Reported extended format"),
        (ChannelCount, "Channel count"),
        (ChannelLayout, "Channel layout"),
        (Bitrate, "Bitrate (bits/s)"),
        (SampleRate, "Sample rate (Hz)"),
        (BitDepth, "Bit depth"),
        (Width, "Width"),
        (Height, "Height"),
        (Resolution, "Resolution"),
        (HdrFormat, "HDR format"),
        (DolbyVisionProfile, "Reported Dolby Vision profile"),
        (ExistingTitle, "Existing title"),
        (Language, "Language tag"),
        (LanguageName, "Language name"),
        (TrackPosition, "Original track position"),
        (Default, "Default flag"),
        (Forced, "Forced flag"),
        (Enabled, "Enabled flag"),
        (HearingImpaired, "Hearing-impaired flag"),
        (Commentary, "Commentary flag"),
        (Original, "Original-language flag"),
        (VisualImpaired, "Visual-impaired flag"),
        (TextDescriptions, "Text-descriptions flag"),
        (Lossless, "Reported lossless status"),
    ];
    let variables = [
        (
            "language_display",
            "Language name with recorded region/script; Unknown if unspecified",
        ),
        (
            "naming_codec",
            "Standard codec/format label from inspection",
        ),
        (
            "channel_suffix",
            "Space + reported layout; count + ch fallback; empty if unknown",
        ),
        (
            "immersive_suffix",
            " + Atmos or + DTS:X when confirmed; otherwise empty",
        ),
        (
            "audio_role_suffix",
            "Parenthesised known audio roles; retains explicit commentary details",
        ),
        (
            "subtitle_qualifiers",
            "Comma-prefixed explicit subtitle qualifiers; empty for ordinary subtitles",
        ),
        ("codec", "Codec"),
        ("codec_profile", "Codec profile"),
        ("extended_format", "Reported extended format"),
        ("channel_count", "Channel count (not an inferred layout)"),
        ("channels", "Channel count alias"),
        ("channel_layout", "Reported channel layout"),
        ("bitrate", "Bitrate in bits/s"),
        ("sample_rate", "Sample rate in Hz"),
        ("bit_depth", "Bit depth"),
        ("resolution", "Resolution"),
        ("width", "Width"),
        ("height", "Height"),
        ("hdr_format", "Reported HDR format"),
        ("dolby_vision_profile", "Reported Dolby Vision profile"),
        ("language", "Language tag"),
        ("language_name", "Primary language name"),
        ("track_position", "Original track position"),
        ("filename", "Filename"),
        ("filename_stem", "Filename without extension"),
        ("container", "Container format"),
        (
            "preferred_language",
            "Profile language fallback (user configured)",
        ),
        (
            "preferred_language_name",
            "Profile language name (user configured)",
        ),
    ];
    RuleCatalog {
        fields: fields
            .into_iter()
            .map(|(field, label)| FieldDefinition {
                field,
                label: label.into(),
                kind: field_kind(field).into(),
                operators: operators(field),
            })
            .collect(),
        variables: variables
            .into_iter()
            .map(|(name, label)| TemplateVariable {
                name: name.into(),
                label: label.into(),
            })
            .collect(),
    }
}
