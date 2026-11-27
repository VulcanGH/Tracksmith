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
        mapping: "uid_number_type_verified".into(),
    };
    Inspection {
        analysis: Default::default(),
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
        sections_pending: false,
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

fn assert_stable_track_title(mut doc: Inspection, before: &str, expected: &str) {
    let library = RuleLibrary::default();
    doc.tracks.truncate(1);
    doc.tracks[0].title = Some(before.into());
    let preview = evaluate(&doc, &library, &request()).unwrap();
    if before == expected {
        assert!(preview.proposals.is_empty(), "Already correct: {before}");
    } else {
        assert_eq!(preview.proposals.len(), 1, "Input: {before}");
        let edit = &preview.proposals[0].edit;
        assert_eq!(edit.track_uid.as_deref(), Some("1"));
        assert_eq!(edit.property, Property::Title);
        assert_eq!(edit.value, json!(expected), "Input: {before}");
        doc.tracks[0].title = edit.value.as_str().map(str::to_owned);
    }
    // Simulate subsequent save/reopen cycles, not just staged rule passes.
    for cycle in 1..=5 {
        let preview = evaluate(&doc, &library, &request()).unwrap();
        assert!(preview.proposals.is_empty(), "Cycle {cycle}: {expected}");
    }
}

#[test]
fn subtitle_commentary_titles_are_stable_for_all_standard_formats() {
    for (code, format) in [
        ("S_TEXT/UTF8", "SRT"),
        ("S_HDMV/PGS", "PGS"),
        ("S_TEXT/ASS", "ASS"),
        ("S_TEXT/SSA", "SSA"),
        ("S_VOBSUB", "VobSub"),
        ("S_TEXT/WEBVTT", "WebVTT"),
        ("S_DVBSUB", "DVB"),
    ] {
        let mut doc = fixture();
        doc.tracks[0].kind = "subtitles".into();
        doc.tracks[0].codec = format.into();
        doc.tracks[0].technical = json!({"codec_id": code});
        doc.tracks[0].language_ietf = Some("en".into());
        let plain = format!("English ({format}, Commentary)");
        assert_stable_track_title(doc.clone(), "Commentary", &plain);
        assert_stable_track_title(doc.clone(), &plain, &plain);
        assert_stable_track_title(
            doc.clone(),
            &format!("English ({format}, Commentary — {format},)"),
            &plain,
        );
        assert_stable_track_title(
            doc.clone(),
            &format!("English ({format}, Commentary — {format})"),
            &plain,
        );
        assert_stable_track_title(
            doc.clone(),
            "Director Commentary",
            &format!("English ({format}, Commentary — Director)"),
        );
        assert_stable_track_title(
            doc.clone(),
            &format!("English ({format}, Forced, SDH, CC, Commentary, Signs & Songs)"),
            &format!("English ({format}, Forced, SDH, CC, Commentary, Signs & Songs)"),
        );
        assert_stable_track_title(
            doc.clone(),
            &format!("English ({format}, Forced, Commentary — {format}, Forced,)"),
            &format!("English ({format}, Forced, Commentary)"),
        );
        doc.tracks[0].flags.insert("commentary".into(), Some(true));
        assert_stable_track_title(doc.clone(), "", &plain);
    }
}

#[test]
fn named_commentary_preserves_unicode_commas_and_balanced_parentheses() {
    let mut doc = fixture();
    doc.tracks[0].kind = "subtitles".into();
    doc.tracks[0].codec = "PGS".into();
    doc.tracks[0].technical = json!({"codec_id": "S_HDMV/PGS"});
    doc.tracks[0].language_ietf = Some("en".into());
    for (before, expected) in [
        ("İpek Commentary", "English (PGS, Commentary — İpek)"),
        (
            "Commentary by Jane, John",
            "English (PGS, Commentary — Jane, John)",
        ),
        (
            "Commentary - Director (Jane, John)",
            "English (PGS, Commentary — Director (Jane, John))",
        ),
        (
            "Director (Jane, John) Commentary",
            "English (PGS, Commentary — Director (Jane, John))",
        ),
        (
            "Commentary by Director (Commentary)",
            "English (PGS, Commentary — Director (Commentary))",
        ),
        (
            "Director (Commentary by Anne) Commentary",
            "English (PGS, Commentary — Director (Commentary by Anne))",
        ),
        (
            "English (PGS, Commentary — Director, Producer, Signs & Songs)",
            "English (PGS, Commentary — Director, Producer, Signs & Songs)",
        ),
        (
            "English (PGS, Commentary — PGS expert)",
            "English (PGS, Commentary — PGS expert)",
        ),
        (
            "English (PGS, Commentary — Commentary by Jane)",
            "English (PGS, Commentary — Commentary by Jane)",
        ),
        ("English (PGS, Commentary — )", "English (PGS, Commentary)"),
    ] {
        assert_stable_track_title(doc.clone(), before, expected);
    }
    doc.tracks[0].language_ietf = Some("ja".into());
    assert_stable_track_title(
        doc,
        "Japanese (PGS, Commentary)",
        "Japanese (PGS, Commentary)",
    );
}

#[test]
fn audio_commentary_and_description_roles_are_stable() {
    let mut doc = fixture();
    doc.tracks[0].language_ietf = Some("en".into());
    for (before, expected) in [
        ("Commentary", "English FLAC 2ch (Commentary)"),
        (
            "Director Commentary",
            "English FLAC 2ch (Commentary — Director)",
        ),
        (
            "English FLAC 2ch (Commentary)",
            "English FLAC 2ch (Commentary)",
        ),
        (
            "English FLAC 2ch (Audio Description, Commentary)",
            "English FLAC 2ch (Commentary, Audio Description)",
        ),
        (
            "English FLAC 2ch (Commentary — Audio Description,, Audio Description)",
            "English FLAC 2ch (Commentary, Audio Description)",
        ),
        (
            "English FLAC 2ch (Commentary — Director (Jane, John), Audio Description)",
            "English FLAC 2ch (Commentary — Director (Jane, John), Audio Description)",
        ),
    ] {
        assert_stable_track_title(doc.clone(), before, expected);
    }
    doc.tracks[0].flags.insert("commentary".into(), Some(true));
    doc.tracks[0]
        .flags
        .insert("visual_impaired".into(), Some(true));
    assert_stable_track_title(
        doc.clone(),
        "",
        "English FLAC 2ch (Commentary, Audio Description)",
    );
    doc.tracks[0].codec = "TrueHD".into();
    doc.tracks[0].technical = json!({"codec_id": "A_TRUEHD", "audio_channels": 8});
    doc.tracks[0].mediainfo =
        Some(json!({"Format_Commercial_IfAny": "Dolby TrueHD with Dolby Atmos"}));
    assert_stable_track_title(
        doc,
        "Director Commentary",
        "English Dolby TrueHD 8ch + Atmos (Commentary — Director, Audio Description)",
    );
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
    assert!(!super::needs_full_media_info(&library, "general").unwrap());
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
    assert!(super::needs_full_media_info(&library, "general").unwrap());
    let mut audio_only = fixture();
    audio_only.tracks.retain(|track| track.kind == "audio");
    assert_eq!(
        super::requirements_for(&library, "general", &audio_only).unwrap(),
        (false, false)
    );
    let mut video = audio_only.tracks[0].clone();
    video.kind = "video".into();
    audio_only.tracks.push(video);
    assert_eq!(
        super::requirements_for(&library, "general", &audio_only).unwrap(),
        (true, true)
    );
}

#[test]
fn incomplete_technical_analysis_preserves_titles_and_allows_container_rules() {
    let mut doc = fixture();
    doc.tracks.truncate(1);
    doc.title = Some("Old container".into());
    doc.tracks[0].title = Some("English Dolby Digital Plus 5.1 + Atmos".into());
    doc.tracks[0].codec = "E-AC-3".into();
    doc.tracks[0].technical = json!({"codec_id":"A_EAC3","audio_channels":6});
    doc.tracks[0].mediainfo = Some(json!({"Format":"E-AC-3","Channels":"6"}));
    for status in ["pending", "failed"] {
        doc.analysis.status = status.into();
        let preview = evaluate(&doc, &RuleLibrary::default(), &request()).unwrap();
        assert_eq!(preview.proposals.len(), 1);
        assert!(preview.proposals[0].edit.track_uid.is_none());
        assert!(
            preview
                .notes
                .iter()
                .any(|note| note.contains("verified metadata"))
        );
    }
    doc.analysis.status = "partial".into();
    doc.tracks[0].mapping = "unavailable".into();
    let preview = evaluate(&doc, &RuleLibrary::default(), &request()).unwrap();
    assert_eq!(preview.proposals.len(), 1);
    doc.tracks[0].mapping = "uid_number_type_verified".into();
    doc.tracks[0].mediainfo.as_mut().unwrap()["Tracksmith_Incomplete"] = json!(true);
    let preview = evaluate(&doc, &RuleLibrary::default(), &request()).unwrap();
    assert_eq!(preview.proposals.len(), 1);
    assert!(preview.proposals[0].edit.track_uid.is_none());
}

#[test]
fn dtsx_object_marker_does_not_count_as_a_physical_channel() {
    let mut doc = fixture();
    doc.tracks.truncate(1);
    doc.tracks[0].language_ietf = Some("en".into());
    doc.tracks[0].codec = "DTS-HD Master Audio".into();
    doc.tracks[0].technical = json!({"codec_id":"A_DTS","audio_channels":8});
    doc.tracks[0].mediainfo = Some(json!({
        "Format":"DTS", "Format_AdditionalFeatures":"XLL X", "Channels":"8",
        "ChannelLayout":"C L R LFE Lb Rb Lss Rss Objects"
    }));
    let preview = evaluate(&doc, &RuleLibrary::default(), &request()).unwrap();
    assert_eq!(
        preview.proposals[0].edit.value,
        "English DTS-HD Master Audio 7.1 + DTS:X"
    );
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
    let uncertain = evaluate(&doc, &library, &request()).unwrap();
    assert!(uncertain.proposals.is_empty());
    assert!(
        uncertain
            .notes
            .iter()
            .any(|note| note.contains("immersive audio label"))
    );
    assert_eq!(super::naming::immersive(&doc.tracks[0]), "");
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
