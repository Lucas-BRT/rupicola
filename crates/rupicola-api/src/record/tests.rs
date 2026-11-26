use super::*;
use crate::CaptureStatus;
use serde_json::json;

const ID: &str = "01936f2c-aaaa-7bbb-8ccc-000000000001";
const PARENT: &str = "01936f2c-aaaa-7bbb-8ccc-000000000000";
const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
/// The canonical form of [`record`], written out by hand.
const CANONICAL: &str = concat!(
    r#"{"captured_at":1795000000000,"environment":{"appearance":{"color_scheme":"dark"},"#,
    r#""display":{"height":915,"orientation":"portrait","scale_milli":2625,"width":412},"#,
    r#""extra":{"android.api_level":"34"},"platform":{"family":"android"}},"format":1,"#,
    r#""id":"01936f2c-aaaa-7bbb-8ccc-000000000001","kind":"scenario","#,
    r#""members":[{"bytes":10,"mime":"image/png","owner":"home","path":"checkpoints/home.png","#,
    r#""role":"checkpoint","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","#,
    r#""surface":"home"},"#,
    r#"{"bytes":20,"mime":"image/png","owner":"open","path":"clips/open/003.png","#,
    r#""role":"clip-frame","sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","#,
    r#""target_ms":300,"video_ms":300}],"#,
    r#""parents":[{"id":"01936f2c-aaaa-7bbb-8ccc-000000000000","kind":"previous"}],"#,
    r#""project":"demo","scenario":{"name":"Settings"},"series":"Settings@a1b2c3","status":"complete"}"#,
);
/// SHA-256 of [`CANONICAL`], computed outside this crate.
const DIGEST: &str = "d0c3fec2457872a0ece7588a177d8522b40e6ae324a12400e9d22785335b34a0";

fn record() -> CaptureRecord {
    CaptureRecord {
        format: RECORD_FORMAT,
        id: ID.to_owned(),
        project: "demo".to_owned(),
        kind: RecordKind::Scenario,
        scenario: ScenarioRef {
            name: "Settings".to_owned(),
            sha256: None,
        },
        revision: None,
        series: "Settings@a1b2c3".to_owned(),
        parents: vec![ParentRef {
            id: PARENT.to_owned(),
            kind: ParentKind::Previous,
        }],
        captured_at: 1_795_000_000_000,
        status: CaptureStatus::Complete,
        blocker: None,
        environment: Environment {
            platform: Some(Platform {
                family: Some(PlatformFamily::Android),
                os_version: None,
            }),
            display: Some(Display {
                width: Some(412),
                height: Some(915),
                scale_milli: Some(2625),
                orientation: Some("portrait".to_owned()),
            }),
            appearance: Some(Appearance {
                color_scheme: Some("dark".to_owned()),
                ..Appearance::default()
            }),
            extra: BTreeMap::from([("android.api_level".to_owned(), "34".to_owned())]),
            ..Environment::default()
        },
        plan_spec: None,
        members: vec![
            Member {
                path: "checkpoints/home.png".to_owned(),
                sha256: SHA_A.to_owned(),
                bytes: 10,
                mime: "image/png".to_owned(),
                role: MemberRole::Checkpoint,
                owner: "home".to_owned(),
                surface: Some("home".to_owned()),
                locator: BTreeMap::new(),
                target_ms: None,
                video_ms: None,
            },
            Member {
                path: "clips/open/003.png".to_owned(),
                sha256: SHA_B.to_owned(),
                bytes: 20,
                mime: "image/png".to_owned(),
                role: MemberRole::ClipFrame,
                owner: "open".to_owned(),
                surface: None,
                locator: BTreeMap::new(),
                target_ms: Some(300),
                video_ms: Some(300),
            },
        ],
    }
}

fn broken(change: impl FnOnce(&mut CaptureRecord)) -> String {
    let mut record = record();
    change(&mut record);
    record.validate().unwrap_err().to_string()
}

#[test]
fn the_canonical_form_is_the_one_written_by_hand() {
    assert_eq!(record().canonical().unwrap(), CANONICAL);
}

#[test]
fn the_digest_is_the_sha256_of_that_form_and_never_changes() {
    assert_eq!(record().digest().unwrap(), DIGEST);
}

#[test]
fn key_order_and_spacing_do_not_change_the_canonical_text() {
    let one = json!({"b": 1, "a": {"y": [1, 2], "x": "é"}});
    let two: Value =
        serde_json::from_str(r#"{ "a" : { "x" : "é", "y" : [ 1 , 2 ] }, "b" : 1 }"#).unwrap();
    assert_eq!(canonical_json(&one).unwrap(), canonical_json(&two).unwrap());
    assert_eq!(
        canonical_json(&one).unwrap(),
        r#"{"a":{"x":"é","y":[1,2]},"b":1}"#
    );
}

#[test]
fn a_number_with_a_fraction_or_an_exponent_has_no_canonical_text() {
    for text in ["2.625", "1e3", "-0.0"] {
        let value: Value = serde_json::from_str(&format!(r#"{{"scale":{text}}}"#)).unwrap();
        assert_eq!(
            canonical_json(&value).unwrap_err(),
            RecordError::NotWhole("$.scale".to_owned()),
            "{text}"
        );
    }
}

#[test]
fn a_record_survives_a_round_trip_with_the_same_digest() {
    let text = serde_json::to_string(&record()).unwrap();
    let back: CaptureRecord = serde_json::from_str(&text).unwrap();
    assert_eq!(back, record());
    assert_eq!(back.digest().unwrap(), record().digest().unwrap());
}

#[test]
fn any_change_to_the_record_changes_the_digest() {
    let mut other = record();
    other.members[1].target_ms = Some(301);
    assert_ne!(other.digest().unwrap(), record().digest().unwrap());
}

#[test]
fn an_unknown_field_is_refused_rather_than_ignored() {
    let mut value = serde_json::to_value(record()).unwrap();
    value["surprise"] = json!(1);
    assert!(serde_json::from_value::<CaptureRecord>(value).is_err());
}

#[test]
fn the_environment_flattens_to_searchable_facets() {
    let facets = record().environment.facets();
    assert_eq!(facets["platform.family"], "android");
    assert_eq!(facets["display.scale_milli"], "2625");
    assert_eq!(facets["appearance.color_scheme"], "dark");
    assert_eq!(facets["extra.android.api_level"], "34");
    assert!(!facets.contains_key("device.model"));
}

#[test]
fn the_rules_of_the_format_are_checked() {
    assert!(broken(|r| r.format = 2).contains("format"));
    assert!(broken(|r| r.id = String::new()).contains("id"));
    assert!(broken(|r| r.parents[0].id = ID.to_owned()).contains("own parent"));
    assert!(broken(|r| r.parents.push(r.parents[0].clone())).contains("twice"));
    assert!(broken(|r| r.captured_at = -1).contains("negative"));
    assert!(broken(|r| r.status = CaptureStatus::Blocked).contains("blocker"));
    assert!(broken(|r| r.blocker = Some("x".to_owned())).contains("no blocker"));
    assert!(broken(|r| r.members.clear()).contains("at least one file"));
    assert!(broken(|r| r.members.reverse()).contains("sorted"));
    assert!(broken(|r| r.members[0].path = "../x.png".to_owned()).contains("path"));
    assert!(broken(|r| r.members[0].path = "a/b.exe".to_owned()).contains("path"));
    assert!(broken(|r| r.members[0].sha256 = "ABC".to_owned()).contains("SHA-256"));
    assert!(broken(|r| r.members[0].target_ms = Some(1)).contains("target_ms"));
    assert!(broken(|r| r.members[1].target_ms = None).contains("target_ms"));
    assert!(broken(|r| r.members[0].video_ms = Some(1)).contains("video_ms"));
    assert!(
        broken(|r| {
            r.environment
                .extra
                .insert("Bad Key".to_owned(), "v".to_owned());
        })
        .contains("key")
    );
    assert!(
        broken(|r| {
            r.environment.display.as_mut().unwrap().width = Some(0);
        })
        .contains("zero")
    );
}

#[test]
fn a_blocked_capture_with_a_blocker_and_no_files_is_valid() {
    let mut blocked = record();
    blocked.status = CaptureStatus::Blocked;
    blocked.blocker = Some("ffmpeg is missing".to_owned());
    blocked.members.clear();
    blocked.validate().unwrap();
}
