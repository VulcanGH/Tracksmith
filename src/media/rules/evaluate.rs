use super::{
    facts::{ContextData, Overlay, canonical_language},
    model::*,
    validate::{bounded_regex, template_parts, validate, validate_rule},
};
use crate::media::model::{Edit, Inspection, Property};
use anyhow::{Context, Result, ensure};
use regex::Regex;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

struct Compiled {
    regex: HashMap<(String, bool), Regex>,
    templates: HashMap<String, Vec<(bool, String)>>,
    budget: usize,
}
impl Compiled {
    fn tick(&mut self) -> Result<()> {
        ensure!(
            self.budget > 0,
            "Rules exceeded 100,000 evaluation steps; narrow the scopes or conditions"
        );
        self.budget -= 1;
        Ok(())
    }
    fn group(&mut self, group: &ConditionGroup, context: &ContextData<'_>) -> Result<bool> {
        let all = group.mode == MatchMode::All;
        for condition in &group.conditions {
            let matched = self.condition(condition, context)?;
            if matched != all {
                return Ok(!all);
            }
        }
        for nested in &group.groups {
            let matched = self.group(nested, context)?;
            if matched != all {
                return Ok(!all);
            }
        }
        Ok(all)
    }
    fn condition(&mut self, condition: &Condition, context: &ContextData<'_>) -> Result<bool> {
        self.tick()?;
        let fact = context.field(condition.field);
        if condition.operator == Operator::IsUndefined {
            return Ok(fact.undefined);
        }
        if condition.operator == Operator::IsEmpty {
            return Ok(fact.value.as_ref().is_some_and(|v| v.as_str() == Some(""))
                || (condition.field == Field::ExistingTitle && fact.value.is_none()));
        }
        if condition.operator == Operator::IsNotEmpty {
            return Ok(fact
                .value
                .as_ref()
                .is_some_and(|v| v.as_str().is_some_and(|s| !s.is_empty())));
        }
        let Some(value) = fact.value else {
            return Ok(false);
        }; // Unknown never means a known negative.
        if let (Some(left), Some(right)) = (value.as_f64(), condition.value.as_f64()) {
            return Ok(match condition.operator {
                Operator::Equals => left == right,
                Operator::DoesNotEqual => left != right,
                Operator::GreaterThan => left > right,
                Operator::LessThan => left < right,
                _ => false,
            });
        }
        if let (Some(left), Some(right)) = (value.as_bool(), condition.value.as_bool()) {
            return Ok(match condition.operator {
                Operator::Equals => left == right,
                Operator::DoesNotEqual => left != right,
                _ => false,
            });
        }
        let (Some(left), Some(right)) = (value.as_str(), condition.value.as_str()) else {
            return Ok(false);
        };
        ensure!(left.len() <= 64 * 1024, "A condition input exceeds 64 KiB");
        if condition.operator == Operator::Regex {
            let key = (right.to_string(), condition.case_sensitive);
            if !self.regex.contains_key(&key) {
                self.regex.insert(key.clone(), bounded_regex(condition)?);
            }
            return Ok(self.regex[&key].is_match(left));
        }
        let right = if condition.field == Field::Language
            && matches!(
                condition.operator,
                Operator::Equals | Operator::DoesNotEqual
            ) {
            canonical_language(right).unwrap_or_else(|_| right.into())
        } else {
            right.into()
        };
        let (left, right) = if condition.case_sensitive {
            (left.to_string(), right)
        } else {
            (left.to_lowercase(), right.to_lowercase())
        };
        Ok(match condition.operator {
            Operator::Equals => left == right,
            Operator::DoesNotEqual => left != right,
            Operator::Contains => left.contains(&right),
            Operator::DoesNotContain => !left.contains(&right),
            Operator::StartsWith => left.starts_with(&right),
            Operator::EndsWith => left.ends_with(&right),
            _ => false,
        })
    }
    fn action(&mut self, action: &Action, context: &ContextData<'_>) -> Result<Option<Value>> {
        self.tick()?;
        match action.source {
            ValueSource::Clear => Ok(Some(Value::Null)),
            ValueSource::FirstAudioLanguage => {
                let language = context
                    .doc
                    .tracks
                    .iter()
                    .position(|track| track.kind == "audio")
                    .and_then(|index| {
                        ContextData {
                            target: Some(index),
                            ..*context
                        }
                        .property(Property::Language)
                        .value
                    })
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .filter(|language| !language.is_empty() && language != "und")
                    .unwrap_or_else(|| "en".into());
                Ok(Some(Value::String(canonical_language(&language)?)))
            }
            ValueSource::PreferredLanguage => Ok(context
                .profile
                .preferred_languages
                .first()
                .map(|v| canonical_language(v))
                .transpose()?
                .map(Value::String)),
            ValueSource::Literal => Ok(Some(if action.property == Property::Language {
                Value::String(canonical_language(
                    action.value.as_str().context("Invalid language action")?,
                )?)
            } else {
                action.value.clone()
            })),
            ValueSource::Template => {
                let template = action.value.as_str().context("Invalid template")?;
                if !self.templates.contains_key(template) {
                    self.templates
                        .insert(template.into(), template_parts(template)?);
                }
                let mut result = String::new();
                for (variable, value) in &self.templates[template] {
                    if *variable {
                        let Some(value) = context.variable(value) else {
                            return Ok(None);
                        };
                        result.push_str(&value);
                    } else {
                        result.push_str(value);
                    }
                    ensure!(result.len() <= 4096, "Resolved title exceeds 4096 bytes");
                }
                ensure!(
                    !result.trim().is_empty(),
                    "Resolved title is empty; use an explicit Clear title action"
                );
                Ok(Some(Value::String(result)))
            }
        }
    }
}
fn matches_scope(scope: Scope, track: Option<&crate::media::model::Track>) -> bool {
    match (scope, track) {
        (Scope::Container, None) => true,
        (Scope::Tracks, Some(_)) => true,
        (Scope::Video, Some(t)) => t.kind == "video",
        (Scope::Audio, Some(t)) => t.kind == "audio",
        (Scope::Subtitles, Some(t)) => t.kind == "subtitles",
        (Scope::Other, Some(t)) => !matches!(t.kind.as_str(), "video" | "audio" | "subtitles"),
        _ => false,
    }
}
fn capabilities<'a>(doc: &'a Inspection, key: &EditKey) -> Result<&'a [Property]> {
    if let Some(uid) = &key.track_uid {
        Ok(&doc
            .tracks
            .iter()
            .find(|t| &t.uid == uid)
            .context("Unknown manual-override track UID")?
            .editable)
    } else {
        Ok(&doc.editable)
    }
}
fn note(notes: &mut Vec<String>, seen: &mut HashSet<String>, value: String) {
    if seen.insert(value.clone()) && notes.len() < 200 {
        notes.push(value);
    } else if notes.len() == 200 {
        notes.push("Additional rule notes were omitted".into());
    }
}

pub fn evaluate(
    doc: &Inspection,
    library: &RuleLibrary,
    request: &RulePreviewRequest,
) -> Result<RulePreview> {
    validate(library)?;
    let profile = library
        .profiles
        .iter()
        .find(|p| p.id == request.profile_id)
        .context("Profile does not exist")?;
    let mut testing = request.test_rule.clone();
    if let Some(rule) = &mut testing {
        validate_rule(rule)?;
        rule.enabled = true;
    }
    let mut rules: Vec<_> = if let Some(rule) = &testing {
        vec![rule]
    } else {
        library
            .rules
            .iter()
            .filter(|r| r.enabled && profile.rule_ids.contains(&r.id))
            .collect()
    };
    rules.sort_by(|a, b| (a.priority, &a.id).cmp(&(b.priority, &b.id)));
    ensure!(
        request.manual_edits.len() <= 4096,
        "Too many manual overrides"
    );
    let mut overlay = Overlay::new();
    let mut locked = BTreeSet::new();
    for edit in &request.manual_edits {
        let key = EditKey {
            track_uid: edit.track_uid.clone(),
            property: edit.property,
        };
        ensure!(locked.insert(key.clone()), "Duplicate manual override");
        ensure!(
            capabilities(doc, &key)?.contains(&edit.property),
            "Manual override targets a read-only property"
        );
        let original = ContextData {
            doc,
            profile,
            overlay: &Overlay::new(),
            target: key
                .track_uid
                .as_ref()
                .and_then(|uid| doc.tracks.iter().position(|t| &t.uid == uid)),
        }
        .property(edit.property)
        .value
        .unwrap_or(Value::Null);
        let value = match edit.property {
            Property::Title => {
                ensure!(
                    edit.value.is_null()
                        || edit
                            .value
                            .as_str()
                            .is_some_and(|v| v.len() <= 4096 && !v.contains('\0')),
                    "Invalid manual title"
                );
                edit.value.clone()
            }
            Property::Language => {
                if edit.value.is_null() && original.is_null() {
                    Value::Null
                } else {
                    Value::String(canonical_language(
                        edit.value.as_str().context("Invalid manual language")?,
                    )?)
                }
            }
            _ => {
                ensure!(
                    edit.value.is_boolean() || (edit.value.is_null() && original.is_null()),
                    "Invalid manual flag"
                );
                edit.value.clone()
            }
        };
        overlay.insert(key, value);
    }
    let empty = Overlay::new();
    let mut compiled = Compiled {
        regex: HashMap::new(),
        templates: HashMap::new(),
        budget: 100_000,
    };
    let mut states = HashSet::new();
    states.insert(serde_json::to_string(&overlay.iter().collect::<Vec<_>>())?);
    let mut origins = BTreeMap::new();
    let mut conflicts = vec![];
    let mut conflict_keys = HashSet::new();
    let mut notes = vec![];
    let mut seen_notes = HashSet::new();
    let mut preserved = BTreeSet::new();
    let mut settled = false;
    let mut passes = 0;
    for pass in 1..=8 {
        passes = pass;
        let mut winners: BTreeMap<EditKey, RuleProposal> = BTreeMap::new();
        for rule in &rules {
            for target in std::iter::once(None).chain((0..doc.tracks.len()).map(Some)) {
                let context = ContextData {
                    doc,
                    profile,
                    overlay: &overlay,
                    target,
                };
                if !matches_scope(rule.scope, context.track()) {
                    continue;
                }
                if context.track().is_some_and(|t| {
                    t.mapping != "uid_number_type_verified"
                        || t.mediainfo
                            .as_ref()
                            .is_some_and(|v| v["Tracksmith_Incomplete"] == true)
                        || matches!(doc.analysis.status.as_str(), "pending" | "failed")
                }) && super::rule_needs_media_info(rule)?
                {
                    note(&mut notes, &mut seen_notes,
                        "Technical naming needs verified metadata; unconfirmed track titles were left unchanged. Recheck this file or open it in Editor.".into());
                    continue;
                }
                compiled.tick()?;
                if !compiled.group(&rule.conditions, &context)? {
                    continue;
                }
                if rule.overwrite == OverwritePolicy::Conditional
                    && !compiled.group(
                        rule.replace_when
                            .as_ref()
                            .context("Missing replacement conditions")?,
                        &context,
                    )?
                {
                    continue;
                }
                let mut actions = vec![];
                let mut skip = false;
                for action in &rule.actions {
                    let key = context.key(action.property);
                    if locked.contains(&key) {
                        preserved.insert(key);
                        continue;
                    }
                    if !capabilities(doc, &key)?.contains(&action.property) {
                        note(
                            &mut notes,
                            &mut seen_notes,
                            format!(
                                "{}: {:?} is read-only on {:?}",
                                rule.name, action.property, key.track_uid
                            ),
                        );
                        continue;
                    }
                    let fact = context.property(action.property);
                    let allowed = match profile.overwrite_policy.unwrap_or(rule.overwrite) {
                        OverwritePolicy::Always | OverwritePolicy::Conditional => true,
                        OverwritePolicy::WhenEmpty => fact.empty(),
                        OverwritePolicy::WhenUndefined => fact.undefined,
                        OverwritePolicy::PreserveExisting => fact.value.is_none(),
                    };
                    if !allowed {
                        continue;
                    }
                    let Some(value) = compiled.action(action, &context)? else {
                        note(
                            &mut notes,
                            &mut seen_notes,
                            format!(
                                "{}: a template variable or configured preferred language is unavailable for {:?}; {:?}",
                                rule.name, key.track_uid, action.missing
                            ),
                        );
                        if action.missing == MissingValue::SkipTarget {
                            skip = true;
                            break;
                        } else {
                            continue;
                        }
                    };
                    if action.property == Property::Title
                        && super::rule_needs_media_info(rule)?
                        && let (Some(before), Some(after)) =
                            (fact.value.as_ref().and_then(Value::as_str), value.as_str())
                        && ["atmos", "dts:x"].iter().any(|label| {
                            before.to_lowercase().contains(label)
                                && !after.to_lowercase().contains(label)
                        })
                    {
                        note(&mut notes, &mut seen_notes, "Existing immersive audio label conflicts with detected facts; review it manually.".into());
                        continue;
                    }
                    actions.push((
                        key,
                        RuleProposal {
                            edit: Edit {
                                track_uid: context.key(action.property).track_uid,
                                property: action.property,
                                value,
                            },
                            before: fact.value.unwrap_or(Value::Null),
                            rule_id: rule.id.clone(),
                            rule_name: rule.name.clone(),
                        },
                    ));
                }
                if skip {
                    continue;
                }
                for (key, proposal) in actions {
                    if let Some(winner) = winners.get(&key) {
                        if winner.edit.value != proposal.edit.value && conflicts.len() < 200 {
                            let fingerprint = serde_json::to_string(&(
                                &key,
                                &winner.rule_id,
                                &proposal.rule_id,
                                &winner.edit.value,
                                &proposal.edit.value,
                            ))?;
                            if conflict_keys.insert(fingerprint) {
                                conflicts.push(RuleConflict {
                                    pass,
                                    winner_id: winner.rule_id.clone(),
                                    ignored_id: proposal.rule_id.clone(),
                                    target: key,
                                    winner: winner.rule_name.clone(),
                                    ignored: proposal.rule_name,
                                    winning_value: winner.edit.value.clone(),
                                    ignored_value: proposal.edit.value,
                                });
                            }
                        }
                    } else {
                        winners.insert(key, proposal);
                    }
                }
            }
        }
        let mut changed = false;
        for (key, proposal) in winners {
            let target = key
                .track_uid
                .as_ref()
                .and_then(|uid| doc.tracks.iter().position(|t| &t.uid == uid));
            let current = ContextData {
                doc,
                profile,
                overlay: &overlay,
                target,
            }
            .property(key.property)
            .value
            .unwrap_or(Value::Null);
            if current != proposal.edit.value {
                changed = true;
                overlay.insert(key.clone(), proposal.edit.value.clone());
            }
            origins.insert(key, proposal);
        }
        if !changed {
            settled = true;
            break;
        }
        let frame = serde_json::to_string(&overlay.iter().collect::<Vec<_>>())?;
        ensure!(
            states.insert(frame),
            "Rules cycle between metadata states; refine their conditions or overwrite policies. No rule proposals were staged"
        );
    }
    ensure!(
        settled,
        "Rules did not settle within eight passes. No rule proposals were staged"
    );
    let mut proposals = vec![];
    for target in std::iter::once(None).chain((0..doc.tracks.len()).map(Some)) {
        let context = ContextData {
            doc,
            profile,
            overlay: &empty,
            target,
        };
        for property in Property::ALL {
            let key = context.key(property);
            if locked.contains(&key) {
                continue;
            }
            if let Some(value) = overlay.get(&key) {
                let before = context.property(property).value.unwrap_or(Value::Null);
                if value != &before {
                    let mut proposal = origins
                        .remove(&key)
                        .context("Rule proposal has no origin")?;
                    proposal.before = before;
                    proposal.edit.value = value.clone();
                    proposals.push(proposal);
                }
            }
        }
    }
    ensure!(
        proposals.len() <= 4096,
        "Too many rule proposals; narrow the profile"
    );
    if !profile.preferred_languages.is_empty() {
        note(
            &mut notes,
            &mut seen_notes,
            "Preferred languages are user-configured fallbacks, not detected content languages"
                .into(),
        );
    }
    if conflicts.len() == 200 {
        notes.push("Conflict display is limited to the first 200 conflicts".into());
    }
    if proposals.is_empty() && rules.is_empty() {
        notes.push("No enabled rules are assigned to this profile".into());
    }
    Ok(RulePreview {
        inspection_id: doc.id.clone(),
        profile_id: profile.id.clone(),
        proposals,
        conflicts,
        notes,
        passes,
        preserved_manual: preserved.len(),
    })
}
