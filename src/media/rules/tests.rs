use super::{evaluate, model::*, store::RuleStore, validate::validate};
use crate::media::model::*;
use serde_json::{Value, json};

fn fixture() -> Inspection {
    let section = MetadataSection {
        available: true,
        xml: Some(String::new()),
        tree: None,
        error: None,
    };
    let track = |uid: &str, channels: bool| Track {
        id: uid.parse().unwrap(),
        uid: uid.into(),
        number: uid.into(),
        kind: "audio".into(),
        codec: "FLAC".into(),
        title: None,
        language: Some("und".into()),
        language_ietf: Some("und".into()),
        editable: Property::ALL.to_vec(),
        flags: Default::default(),
        technical: if channels {
            json!({"audio_channels":2})
        } else {
            json!({})
        },
        mediainfo: None,
        mapping: "unconfirmed".into(),
    };
    Inspection {
        id: "fixture".into(),
        path: "/media/fixture.mka".into(),
        size_bytes: "100".into(),
        container: "Matroska".into(),
        title: Some("fixture".into()),
        editable: vec![Property::Title],
        tracks: vec![track("1", true), track("2", false)],
        attachments: json!([]),
        chapters: section.clone(),
        tags: section,
        container_info: json!({}),
        identification: json!({}),
        mediainfo: None,
        warnings: vec![],
        write_available: true,
        elapsed_ms: 0,
    }
}
fn rule(id: &str, priority: u32, action: Action) -> Rule {
    Rule {
        id: id.into(),
        name: id.into(),
        description: String::new(),
        enabled: true,
        priority,
        scope: Scope::Audio,
        conditions: ConditionGroup {
            mode: MatchMode::All,
            conditions: vec![],
            groups: vec![],
        },
        actions: vec![action],
        overwrite: OverwritePolicy::Always,
        replace_when: None,
    }
}
fn action(property: Property, source: ValueSource, value: Value) -> Action {
    Action {
        property,
        source,
        value,
        missing: MissingValue::SkipAction,
    }
}
fn request() -> RulePreviewRequest {
    RulePreviewRequest {
        inspection_id: "fixture".into(),
        profile_id: "general".into(),
        manual_edits: vec![],
        test_rule: None,
        test_library: None,
    }
}

#[test]
fn video_language_uses_first_audio_then_english_and_preserves_defined_values() {
    let library = RuleLibrary::default();
    for (first, second, expected) in [
        (Some("pt-BR"), Some("en"), "pt-BR"),
        (Some("und"), Some("fr"), "en"),
        (None, Some("fr"), "en"),
    ] {
        let mut doc = fixture();
        let mut video = doc.tracks[0].clone();
        video.uid = "3".into();
        video.kind = "video".into();
        video.title = Some("fixture".into());
        doc.tracks[0].language_ietf = first.map(str::to_owned);
        doc.tracks[0].language = first.map(str::to_owned);
        doc.tracks[1].language_ietf = second.map(str::to_owned);
        doc.tracks.insert(0, video);
        for existing in [Some("und"), None, Some("de")] {
            doc.tracks[0].language_ietf = existing.map(str::to_owned);
            doc.tracks[0].language = existing.map(str::to_owned);
            let preview = evaluate(&doc, &library, &request()).unwrap();
            let edit = preview
                .proposals
                .iter()
                .find(|p| p.edit.property == Property::Language);
            if existing == Some("de") {
                assert!(edit.is_none());
            } else {
                assert_eq!(edit.unwrap().edit.value, json!(expected));
            }
        }
    }
    let mut doc = fixture();
    doc.tracks.truncate(1);
    doc.tracks[0].kind = "video".into();
    let preview = evaluate(&doc, &library, &request()).unwrap();
    assert_eq!(
        preview
            .proposals
            .iter()
            .find(|p| p.edit.property == Property::Language)
            .unwrap()
            .edit
            .value,
        json!("en")
    );
    doc.tracks[0].language_ietf = Some("en".into());
    doc.tracks[0].title = Some("fixture".into());
    assert!(
        evaluate(&doc, &library, &request())
            .unwrap()
            .proposals
            .is_empty()
    );
}

#[test]
fn technical_enrichment_is_retained_for_naming_and_nested_custom_rules() {
    use super::needs_media_info;
    let mut library = RuleLibrary::default();
    assert!(needs_media_info(&library, "general").unwrap());
    for rule in &mut library.rules {
        rule.enabled = matches!(rule.scope, Scope::Video | Scope::Container);
    }
    assert!(!needs_media_info(&library, "general").unwrap());
    let rule = library
        .rules
        .iter_mut()
        .find(|r| r.scope == Scope::Video)
        .unwrap();
    rule.conditions.groups.push(ConditionGroup {
        mode: MatchMode::All,
        conditions: vec![Condition {
            field: Field::HdrFormat,
            operator: Operator::Contains,
            value: json!("Dolby"),
            case_sensitive: false,
        }],
        groups: vec![],
    });
    assert!(needs_media_info(&library, "general").unwrap());
}

#[test]
fn standard_naming_uses_confirmed_formats_layouts_roles_and_editable_defaults() {
    let library = RuleLibrary::default();
    validate(&library).unwrap();
    assert!(library.profiles[0].auto_apply);
    assert!(library.rules.iter().all(|r| r.enabled));
    let mut doc = fixture();
    doc.tracks.truncate(1);
    doc.tracks[0].language_ietf = Some("en".into());
    let cases = [
        (
            "A_AC3",
            "AC-3",
            json!({"ChannelLayout":"C","Channels":"1"}),
            "English Dolby Digital 1.0",
        ),
        (
            "A_AC3",
            "AC-3",
            json!({"ChannelLayout":"L R C Ls Rs","Channels":"5"}),
            "English Dolby Digital 5.0",
        ),
        (
            "A_EAC3",
            "E-AC-3",
            json!({"ChannelLayout":"L R C LFE Ls Rs","Channels":"6","Format_AdditionalFeatures":"JOC"}),
            "English Dolby Digital Plus 5.1 + Atmos",
        ),
        (
            "A_TRUEHD",
            "TrueHD",
            json!({"ChannelLayout":"L R C LFE Ls Rs Lb Rb","Channels":"8","Format_Commercial_IfAny":"Dolby TrueHD with Dolby Atmos"}),
            "English Dolby TrueHD 7.1 + Atmos",
        ),
        (
            "A_TRUEHD",
            "TrueHD",
            json!({"Channels":"8"}),
            "English Dolby TrueHD 8ch",
        ),
        ("A_TRUEHD", "TrueHD", json!({}), "English Dolby TrueHD"),
        (
            "A_DTS",
            "DTS",
            json!({"ChannelLayout":"L R C LFE Ls Rs Lb Rb","Channels":"8","Format_AdditionalFeatures":"XLL X"}),
            "English DTS-HD Master Audio 7.1 + DTS:X",
        ),
        (
            "A_DTS",
            "DTS",
            json!({"ChannelLayout":"L R C LFE Ls Rs Lb Rb","Channels":"8","Format_AdditionalFeatures":"XBR"}),
            "English DTS-HD HRA 7.1",
        ),
        (
            "A_DTS",
            "DTS-ES Discrete",
            json!({"ChannelLayout":"L R C LFE Ls Rs Cb","Channels":"7"}),
            "English DTS-ES Discrete 6.1",
        ),
        (
            "A_DTS",
            "DTS-ES Matrix",
            json!({"ChannelLayout":"L R C LFE Ls Rs","Channels":"6"}),
            "English DTS-ES Matrix 5.1",
        ),
        (
            "A_AAC",
            "AAC",
            json!({"ChannelLayout":"L R","Channels":"2","Format_Profile":"LC"}),
            "English AAC-LC 2.0",
        ),
        (
            "A_AAC",
            "AAC",
            json!({"ChannelLayout":"L R","Channels":"2","Format_Profile":"LC / SBR / PS"}),
            "English HE-AAC v2 2.0",
        ),
        (
            "A_AAC",
            "AAC",
            json!({"ChannelLayout":"L R","Channels":"2","Format_Profile":"LC","Format_Settings_SBR":"Yes (Explicit)"}),
            "English HE-AAC 2.0",
        ),
        (
            "A_AAC",
            "AAC",
            json!({"ChannelLayout":"L R","Channels":"2"}),
            "English AAC 2.0",
        ),
        (
            "A_FLAC",
            "FLAC",
            json!({"ChannelLayout":"L R C Ls Rs Cb","Channels":"6"}),
            "English FLAC 6.0",
        ),
        (
            "A_FLAC",
            "FLAC",
            json!({"ChannelLayout":"L R C LFE Ls Rs","Channels":"8"}),
            "English FLAC 8ch",
        ),
        (
            "A_FLAC",
            "FLAC",
            json!({"ChannelLayout":"L R C LFE Ls Rs Lb Rb Tfl Tfr Tbl Tbr","Channels":"12"}),
            "English FLAC 7.1.4",
        ),
        (
            "A_FLAC",
            "FLAC",
            json!({"ChannelLayout":"L R C Bfl Bfr Bfc","Channels":"6"}),
            "English FLAC 6ch",
        ),
        (
            "A_ALAC",
            "ALAC",
            json!({"ChannelLayout":"L R","Channels":"2"}),
            "English ALAC 2.0",
        ),
        (
            "A_PCM/INT/LIT",
            "PCM",
            json!({"ChannelLayout":"L R","Channels":"2"}),
            "English LPCM 2.0",
        ),
        (
            "A_OPUS",
            "Opus",
            json!({"ChannelLayout":"L R C LFE Ls Rs","Channels":"6"}),
            "English Opus 5.1",
        ),
        (
            "A_VORBIS",
            "Vorbis",
            json!({"ChannelLayout":"L R","Channels":"2"}),
            "English Vorbis 2.0",
        ),
        (
            "A_MPEG/L3",
            "MPEG-1 layer 3",
            json!({"ChannelLayout":"L R","Channels":"2"}),
            "English MP3 2.0",
        ),
        (
            "A_MPEG/L2",
            "MPEG-1 layer 2",
            json!({"ChannelLayout":"C","Channels":"1"}),
            "English MP2 1.0",
        ),
    ];
    for (code, raw, mi, expected) in cases {
        let track = &mut doc.tracks[0];
        track.codec = raw.into();
        track.technical = json!({"codec_id":code});
        track.mediainfo = Some(mi);
        track.title = None;
        let preview = evaluate(&doc, &library, &request()).unwrap();
        assert_eq!(preview.proposals.len(), 1, "{raw}: {:?}", preview.notes);
        assert_eq!(preview.proposals[0].edit.value, json!(expected));
        doc.tracks[0].title = Some(expected.into());
        assert!(
            evaluate(&doc, &library, &request())
                .unwrap()
                .proposals
                .is_empty(),
            "not idempotent: {expected}"
        );
    }
    doc.tracks[0].codec = "TrueHD".into();
    doc.tracks[0].technical = json!({"codec_id":"A_TRUEHD","audio_channels":8});
    doc.tracks[0].mediainfo = None;
    doc.tracks[0].title = Some("Dolby Atmos".into());
    assert_eq!(
        evaluate(&doc, &library, &request()).unwrap().proposals[0]
            .edit
            .value,
        json!("English Dolby TrueHD 8ch")
    );
    doc.tracks[0].title = Some("İpek Commentary".into());
    assert_eq!(
        evaluate(&doc, &library, &request()).unwrap().proposals[0]
            .edit
            .value,
        json!("English Dolby TrueHD 8ch (Commentary — İpek)")
    );
    for (code, label) in [
        ("S_TEXT/UTF8", "SRT"),
        ("S_HDMV/PGS", "PGS"),
        ("S_TEXT/ASS", "ASS"),
        ("S_TEXT/SSA", "SSA"),
        ("S_VOBSUB", "VobSub"),
        ("S_TEXT/WEBVTT", "WebVTT"),
    ] {
        let track = &mut doc.tracks[0];
        track.kind = "subtitles".into();
        track.codec = label.into();
        track.technical = json!({"codec_id":code});
        track.title = None;
        track.flags.clear();
        assert_eq!(
            evaluate(&doc, &library, &request()).unwrap().proposals[0]
                .edit
                .value,
            json!(format!("English ({label})"))
        );
    }
    let track = &mut doc.tracks[0];
    track.title = Some("Signs & Songs".into());
    track.flags.insert("forced".into(), Some(true));
    track.flags.insert("hearing_impaired".into(), Some(true));
    assert_eq!(
        evaluate(&doc, &library, &request()).unwrap().proposals[0]
            .edit
            .value,
        json!("English (WebVTT, Forced, SDH, Signs & Songs)")
    );
    doc.tracks[0].title = Some("English (WebVTT, Forced, SDH, Signs & Songs)".into());
    assert!(
        evaluate(&doc, &library, &request())
            .unwrap()
            .proposals
            .is_empty()
    );
    doc.tracks[0].language_ietf = Some("pt-BR".into());
    assert!(
        evaluate(&doc, &library, &request()).unwrap().proposals[0]
            .edit
            .value
            .as_str()
            .unwrap()
            .starts_with("Portuguese (Brazil)")
    );
    doc.tracks[0].technical = json!({"codec_id":"S_UNKNOWN"});
    doc.tracks[0].codec = "Unknown".into();
    assert!(
        evaluate(&doc, &library, &request())
            .unwrap()
            .proposals
            .is_empty()
    );
    doc.path = "/media/Amélie.2001.1080p.mkv".into();
    doc.title = Some("Old container title".into());
    doc.tracks[0].kind = "video".into();
    doc.tracks[0].title = Some("Original video track title".into());
    let movie = evaluate(&doc, &library, &request()).unwrap();
    assert_eq!(movie.proposals.len(), 2);
    assert!(
        movie
            .proposals
            .iter()
            .all(|p| p.edit.value == json!("Amélie.2001.1080p"))
    );
    assert!(movie.proposals.iter().any(|p| p.edit.track_uid.is_none()));
    assert_eq!(doc.title.as_deref(), Some("Old container title"));
    doc.tracks[0].title = Some("Amélie.2001.1080p".into());
    doc.title = Some("Amélie.2001.1080p".into());
    assert!(
        evaluate(&doc, &library, &request())
            .unwrap()
            .proposals
            .is_empty()
    );
    // Saving an intentional empty/custom library must not silently reinstall the standard.
    let dir = tempfile::tempdir().unwrap();
    let store = RuleStore::new(dir.path());
    let mut custom = store.load().unwrap();
    custom.rules.clear();
    custom.profiles[0].rule_ids.clear();
    custom.profiles[0].auto_apply = false;
    store.save(custom).unwrap();
    assert!(store.load().unwrap().rules.is_empty());
}
fn library(rules: Vec<Rule>) -> RuleLibrary {
    let mut library = RuleLibrary::default();
    library.profiles[0].preferred_languages = vec!["en".into()];
    library.profiles[0].rule_ids = rules.iter().map(|r| r.id.clone()).collect();
    library.rules = rules;
    library
}

#[test]
fn rules_settle_with_conflicts_unknowns_missing_templates_and_manual_original_overrides() {
    let mut language = rule(
        "language",
        0,
        action(
            Property::Language,
            ValueSource::PreferredLanguage,
            Value::Null,
        ),
    );
    language.overwrite = OverwritePolicy::WhenUndefined;
    let title = rule(
        "title",
        10,
        action(
            Property::Title,
            ValueSource::Template,
            json!("{language_name} {channel_count}ch"),
        ),
    );
    let lower = rule(
        "lower",
        20,
        action(
            Property::Title,
            ValueSource::Literal,
            json!("Lower priority"),
        ),
    );
    let mut unknown = rule(
        "unknown",
        30,
        action(Property::Forced, ValueSource::Literal, json!(true)),
    );
    unknown.conditions.conditions.push(Condition {
        field: Field::HearingImpaired,
        operator: Operator::DoesNotEqual,
        value: json!(false),
        case_sensitive: false,
    });
    let library = library(vec![language, title, lower, unknown]);
    let doc = fixture();
    let result = evaluate(&doc, &library, &request()).unwrap();
    assert!(
        result
            .proposals
            .iter()
            .any(|p| p.edit.track_uid.as_deref() == Some("1") && p.edit.value == "English 2ch")
    );
    assert!(
        !result
            .proposals
            .iter()
            .any(|p| p.edit.property == Property::Forced)
    );
    assert!(!result.conflicts.is_empty());
    assert!(result.notes.iter().any(|n| n.contains("unavailable")));
    let mut saved = doc.clone();
    for proposal in &result.proposals {
        let track = saved
            .tracks
            .iter_mut()
            .find(|t| Some(t.uid.as_str()) == proposal.edit.track_uid.as_deref())
            .unwrap();
        match proposal.edit.property {
            Property::Title => track.title = proposal.edit.value.as_str().map(str::to_owned),
            Property::Language => {
                track.language_ietf = proposal.edit.value.as_str().map(str::to_owned)
            }
            _ => (),
        }
    }
    assert!(
        evaluate(&saved, &library, &request())
            .unwrap()
            .proposals
            .is_empty()
    );
    let mut manual = request();
    manual.manual_edits.push(Edit {
        track_uid: Some("1".into()),
        property: Property::Title,
        value: Value::Null,
    });
    let protected = evaluate(&doc, &library, &manual).unwrap();
    assert_eq!(protected.preserved_manual, 1);
    assert!(!protected.proposals.iter().any(|p|p.edit.track_uid.as_deref()==Some("1") && p.edit.property==Property::Title));
}
#[test]
fn cyclic_rules_invalid_regex_and_stale_library_revisions_are_rejected() {
    let mut a = rule(
        "a",
        0,
        action(Property::Title, ValueSource::Literal, json!("B")),
    );
    a.conditions.conditions.push(Condition {
        field: Field::ExistingTitle,
        operator: Operator::Equals,
        value: json!("A"),
        case_sensitive: true,
    });
    let mut b = rule(
        "b",
        1,
        action(Property::Title, ValueSource::Literal, json!("A")),
    );
    b.conditions.conditions.push(Condition {
        field: Field::ExistingTitle,
        operator: Operator::Equals,
        value: json!("B"),
        case_sensitive: true,
    });
    let mut doc = fixture();
    doc.tracks[0].title = Some("A".into());
    let mut library = library(vec![a, b]);
    assert!(
        evaluate(&doc, &library, &request())
            .err()
            .unwrap()
            .to_string()
            .contains("cycle")
    );
    library.rules[0].conditions.conditions[0].operator = Operator::Regex;
    library.rules[0].conditions.conditions[0].value = json!("(");
    assert!(validate(&library).is_err());
    let dir = tempfile::tempdir().unwrap();
    let store = RuleStore::new(dir.path());
    let initial = store.load().unwrap();
    assert_eq!(store.save(initial.clone()).unwrap().revision, 1);
    assert!(store.save(initial).is_err());
    assert_eq!(store.load().unwrap().revision, 1);
}
