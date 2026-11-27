mod evaluate;
mod facts;
pub mod model;
mod naming;
mod standard;
mod store;
mod validate;
pub use evaluate::evaluate;
pub use standard::library as standard_library;
pub use store::RuleStore;
pub use validate::{catalog, validate};

/// Whether this rule requires facts beyond container identification.
pub(crate) fn rule_needs_media_info(rule: &model::Rule) -> anyhow::Result<bool> {
    use anyhow::Context;
    use model::{ConditionGroup, Field, Scope, ValueSource};
    fn group_needs_info(group: &ConditionGroup, scope: Scope) -> bool {
        group
            .conditions
            .iter()
            .any(|condition| match condition.field {
                Field::NamingCodec if scope == Scope::Subtitles => false,
                Field::NamingCodec
                | Field::CodecProfile
                | Field::ExtendedFormat
                | Field::ChannelLayout
                | Field::Bitrate
                | Field::BitDepth
                | Field::HdrFormat
                | Field::DolbyVisionProfile
                | Field::Lossless => true,
                _ => false,
            })
            || group.groups.iter().any(|g| group_needs_info(g, scope))
    }
    if group_needs_info(&rule.conditions, rule.scope)
        || rule
            .replace_when
            .as_ref()
            .is_some_and(|g| group_needs_info(g, rule.scope))
    {
        return Ok(true);
    }
    for action in &rule.actions {
        if action.source != ValueSource::Template {
            continue;
        }
        for (variable, name) in
            validate::template_parts(action.value.as_str().context("Invalid template")?)?
        {
            if variable
                && matches!(
                    name.as_str(),
                    "naming_codec"
                        | "codec_profile"
                        | "extended_format"
                        | "channel_layout"
                        | "channel_suffix"
                        | "immersive_suffix"
                        | "bitrate"
                        | "bit_depth"
                        | "hdr_format"
                        | "dolby_vision_profile"
                )
                && !(name == "naming_codec" && rule.scope == Scope::Subtitles)
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Request enrichment only when an enabled profile rule needs it.
pub(crate) fn needs_media_info(
    library: &model::RuleLibrary,
    profile_id: &str,
) -> anyhow::Result<bool> {
    use anyhow::Context;
    let profile = library
        .profiles
        .iter()
        .find(|p| p.id == profile_id)
        .context("Unknown profile")?;
    for rule in library
        .rules
        .iter()
        .filter(|r| r.enabled && profile.rule_ids.contains(&r.id))
    {
        if rule_needs_media_info(rule)? {
            return Ok(true);
        }
    }
    Ok(false)
}
#[cfg(test)]
mod tests;

/// Audio samples cannot provide video facts or whole-stream bitrate.
pub(crate) fn needs_full_media_info(
    library: &model::RuleLibrary,
    profile_id: &str,
) -> anyhow::Result<bool> {
    use anyhow::Context;
    use model::{ConditionGroup, Field, Scope, ValueSource};
    fn full_group(group: &ConditionGroup) -> bool {
        group.conditions.iter().any(|c| {
            matches!(
                c.field,
                Field::Bitrate | Field::HdrFormat | Field::DolbyVisionProfile
            )
        }) || group.groups.iter().any(full_group)
    }
    let profile = library
        .profiles
        .iter()
        .find(|p| p.id == profile_id)
        .context("Unknown profile")?;
    for rule in library
        .rules
        .iter()
        .filter(|r| r.enabled && profile.rule_ids.contains(&r.id))
    {
        if (rule.scope != Scope::Audio && rule_needs_media_info(rule)?)
            || full_group(&rule.conditions)
            || rule.replace_when.as_ref().is_some_and(full_group)
        {
            return Ok(true);
        }
        for action in &rule.actions {
            if action.source == ValueSource::Template
                && validate::template_parts(action.value.as_str().context("Invalid template")?)?
                    .iter()
                    .any(|(variable, name)| {
                        *variable
                            && matches!(
                                name.as_str(),
                                "bitrate" | "hdr_format" | "dolby_vision_profile"
                            )
                    })
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// A rule for absent track types cannot require an extra metadata provider.
pub(crate) fn requirements_for(
    library: &model::RuleLibrary,
    profile_id: &str,
    document: &crate::media::model::Inspection,
) -> anyhow::Result<(bool, bool)> {
    use model::Scope;
    let mut scoped = library.clone();
    scoped.rules.retain(|rule| match rule.scope {
        Scope::Container => true,
        Scope::Tracks => !document.tracks.is_empty(),
        Scope::Audio => document.tracks.iter().any(|t| t.kind == "audio"),
        Scope::Video => document.tracks.iter().any(|t| t.kind == "video"),
        Scope::Subtitles => document.tracks.iter().any(|t| t.kind == "subtitles"),
        Scope::Other => document
            .tracks
            .iter()
            .any(|t| !matches!(t.kind.as_str(), "audio" | "video" | "subtitles")),
    });
    Ok((
        needs_media_info(&scoped, profile_id)?,
        needs_full_media_info(&scoped, profile_id)?,
    ))
}
