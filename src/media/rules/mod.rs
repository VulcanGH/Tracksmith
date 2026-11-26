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

/// Request technical enrichment only when an enabled profile rule uses it.
pub(crate) fn needs_media_info(
    library: &model::RuleLibrary,
    profile_id: &str,
) -> anyhow::Result<bool> {
    use anyhow::Context;
    use model::{ConditionGroup, Field, ValueSource};
    fn group_needs_info(group: &ConditionGroup) -> bool {
        group.conditions.iter().any(|condition| {
            !matches!(
                condition.field,
                Field::Container
                    | Field::HasVideo
                    | Field::Filename
                    | Field::FilePath
                    | Field::TrackType
                    | Field::Codec
                    | Field::Resolution
                    | Field::ExistingTitle
                    | Field::Language
                    | Field::LanguageName
                    | Field::TrackPosition
                    | Field::Default
                    | Field::Forced
                    | Field::Enabled
                    | Field::HearingImpaired
                    | Field::Commentary
                    | Field::Original
                    | Field::VisualImpaired
                    | Field::TextDescriptions
            )
        }) || group.groups.iter().any(group_needs_info)
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
        if group_needs_info(&rule.conditions)
            || rule.replace_when.as_ref().is_some_and(group_needs_info)
        {
            return Ok(true);
        }
        for action in &rule.actions {
            if action.source == ValueSource::Template {
                for (variable, name) in
                    validate::template_parts(action.value.as_str().context("Invalid template")?)?
                {
                    if variable
                        && !matches!(
                            name.as_str(),
                            "language_display"
                                | "audio_role_suffix"
                                | "subtitle_qualifiers"
                                | "codec"
                                | "resolution"
                                | "language"
                                | "language_name"
                                | "track_position"
                                | "filename"
                                | "filename_stem"
                                | "container"
                                | "preferred_language"
                                | "preferred_language_name"
                        )
                    {
                        return Ok(true);
                    }
                }
            }
        }
    }
    Ok(false)
}
#[cfg(test)]
mod tests;
