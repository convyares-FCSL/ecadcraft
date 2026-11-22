//! Revisions as data (S4, #37): the typed `Rev` and its project scheme, named points keyed by kind
//! and name, and the typed revision record.

use std::collections::BTreeMap;

use ecad_model::document::project::parse_date;
use ecad_model::{PointKind, PointName, Project, Rev, RevPrefix, RevScheme, Revision, VersionName};

fn schemes() -> [RevScheme; 3] {
    [
        RevScheme::Letters { dash_first: true },
        RevScheme::Letters { dash_first: false },
        RevScheme::Numbered { prefix: RevPrefix::new("P").ok(), width: 2, start: 1 },
    ]
}

#[test]
fn revs_read_show_and_order_by_scheme() {
    // (scheme, text, rev)
    let table = [
        (0, "\u{2013}", Rev::Initial),
        (0, "-", Rev::Initial),
        (0, "A", Rev::Letter(1)),
        (0, "Z", Rev::Letter(26)),
        (0, "AA", Rev::Letter(27)),
        (1, "A", Rev::Letter(1)),
        (2, "P01", Rev::Number(1)),
        (2, "P10", Rev::Number(10)),
        (2, "P100", Rev::Number(100)),
    ];
    for (s, text, rev) in table {
        let scheme = &schemes()[s];
        assert_eq!(scheme.parse(text), Ok(rev), "{text}");
        assert!(scheme.admits(rev), "{text}");
        // A rev shows as the scheme writes it (a dash is the en dash), and reads back.
        let shown = scheme.show(rev);
        assert_eq!(scheme.parse(&shown), Ok(rev), "{shown}");
    }
    let a = &schemes()[0];
    assert!(a.parse("A").unwrap() < a.parse("B").unwrap() && a.parse("Z").unwrap() < a.parse("AA").unwrap());
    assert!(Rev::Initial < Rev::Letter(1));
}

#[test]
fn next_comes_from_the_scheme() {
    let [dash, from_a, numbered] = schemes();
    assert_eq!(dash.next(None), Rev::Initial);
    assert_eq!(dash.next(Some(Rev::Initial)), Rev::Letter(1));
    assert_eq!(dash.next(Some(Rev::Letter(26))), Rev::Letter(27));
    assert_eq!(from_a.next(None), Rev::Letter(1));
    assert_eq!(numbered.next(None), Rev::Number(1));
    assert_eq!(numbered.show(numbered.next(Some(Rev::Number(9)))), "P10");
    assert_eq!(numbered.show(numbered.next(Some(Rev::Number(1)))), "P02");
}

#[test]
fn a_rev_outside_the_scheme_is_refused_and_the_refusal_names_the_problem() {
    let [dash, from_a, numbered] = schemes();
    // (scheme, text, words the refusal must hold)
    let table: [(&RevScheme, &str, &str); 8] = [
        (&dash, "a", "not a rev of this project"),
        (&dash, "A1", "not a rev of this project"),
        (&dash, "", "empty"),
        (&dash, "ABCD", "not a rev of this project"),
        (&from_a, "-", "start at A"),
        (&numbered, "P1", "P01"),
        (&numbered, "01", "not a rev of this project"),
        (&numbered, "A", "not a rev of this project"),
    ];
    for (scheme, text, words) in table {
        let e = scheme.parse(text).expect_err(text).to_string();
        assert!(e.contains(words), "{text}: {e}");
    }
    // The message names what the revs are.
    assert!(dash.parse("x").unwrap_err().to_string().contains("A, B"));
}

#[test]
fn a_project_with_a_rev_outside_its_scheme_or_twice_is_not_well_formed() {
    let row = |rev: Rev| Revision { rev: Some(rev), issued: true, ..Revision::default() };
    let mut p = Project { revisions: vec![row(Rev::Letter(1)), row(Rev::Letter(2))], ..Project::default() };
    assert_eq!(p.problem(), None);
    p.revisions = vec![row(Rev::Number(1))];
    assert!(p.problem().is_some_and(|m| m.contains("scheme")), "{:?}", p.problem());
    p.revisions = vec![row(Rev::Letter(1)), row(Rev::Letter(1))];
    assert!(p.problem().is_some_and(|m| m.contains("two rows")), "{:?}", p.problem());
    p.revisions = vec![Revision { issued: true, ..Revision::default() }];
    assert!(p.problem().is_some_and(|m| m.contains("without a rev")), "{:?}", p.problem());
}

#[test]
fn a_user_version_b_and_a_revision_b_are_two_points() {
    let user = PointName::Version(VersionName::new("B").unwrap());
    let rev = PointName::Revision(Rev::Letter(2));
    assert_ne!(user, rev);
    assert_eq!((user.kind(), rev.kind()), (PointKind::Version, PointKind::Revision));
    let mut m = BTreeMap::new();
    m.insert(user.clone(), 3u64);
    m.insert(rev.clone(), 7u64);
    assert_eq!(m.len(), 2);
    // A row's kind and key rebuild exactly the point, so a revision never reads as a version.
    for p in [&user, &rev] {
        assert_eq!(PointName::from_row(p.kind(), &p.key()).as_ref(), Some(p));
    }
    // The same key under the other kind is another point (or none).
    assert_eq!(PointName::from_row(PointKind::Revision, "B"), None);
}

#[test]
fn a_date_is_a_calendar_date_and_free_text_is_refused_by_name() {
    assert_eq!(parse_date("2026-11-22").unwrap().to_string(), "2026-11-22");
    for bad in ["next week", "22/11/2026", "2026-13-01", "2026-2-3", ""] {
        let e = parse_date(bad).expect_err(bad).to_string();
        assert!(e.contains("not a date") && e.contains("YYYY-MM-DD"), "{bad}: {e}");
    }
}

#[test]
fn a_revision_record_round_trips_with_an_iso_date_and_typed_rev() {
    let row = Revision {
        rev: Some(Rev::Letter(2)),
        date: Some(parse_date("2026-11-22").unwrap()),
        description: ecad_model::RevValue::new("Second issue").ok(),
        drawn: ecad_model::RevValue::new("CV").ok(),
        ecr: ecad_model::RevValue::new("ECR-114").ok(),
        issued: true,
        ..Revision::default()
    };
    let json = serde_json::to_value(&row).unwrap();
    assert_eq!(json["date"], "2026-11-22");
    assert_eq!(json["rev"], serde_json::json!({"letter": 2}));
    assert_eq!(json["issued"], true);
    assert_eq!(json["ecr"], "ECR-114");
    assert_eq!(serde_json::from_value::<Revision>(json).unwrap(), row);
    // A date that is not a date does not read.
    let bad = serde_json::json!({"date": "tomorrow"});
    assert!(serde_json::from_value::<Revision>(bad).is_err());
}

// ---------------------------------------------------------------- merge (condition 5)

mod merging {
    use super::*;
    use ecad_model::Document;
    use ecad_model::merge::apply;
    use ecad_model::merge::state::{Choice, Inputs, Plan, Scope};
    use ecad_model::naming::tag::HighWater;

    /// A document that issued revision `rev` (described as `what`) at log position `at`.
    fn issued(rev: Rev, at: u64, what: &str) -> Document {
        let mut d = Document::default();
        d.edit_parts(|parts| {
            parts.project.revisions =
                vec![Revision { rev: Some(rev), description: ecad_model::RevValue::new(what).ok(), issued: true, ..Revision::default() }];
        });
        d.edit_parts(|parts| parts.versions.insert(PointName::Revision(rev), at));
        d
    }

    fn plan(a: &Document, t: &Document, s: &Document, choices: &BTreeMap<String, Choice>) -> Plan {
        let high = HighWater::default();
        let scope = Scope { project: true, ..Scope::default() };
        let inp = Inputs {
            ancestor: a,
            target: t,
            source: s,
            scope: &scope,
            target_name: "main",
            source_name: "branch",
            keep_project_key: &|_| true,
            high: &high,
            edit_rule: &|_, _, v| Ok(v.to_string()),
            join: &|_, _, _| Err("no".into()),
        };
        apply::plan(&inp, choices)
    }

    #[test]
    fn the_same_revision_issued_on_both_branches_is_a_named_conflict_never_a_silent_pick() {
        let a = Document::default();
        let (t, s) = (issued(Rev::Letter(2), 3, "from main"), issued(Rev::Letter(2), 5, "from the branch"));
        let p = plan(&a, &t, &s, &BTreeMap::new());
        let c = p.conflicts.iter().find(|c| c.id.ends_with("/revision.issued")).expect("a revision conflict");
        assert!(c.what.contains("Revision B") && c.what.contains("issued on both branches"), "{}", c.what);
        assert_eq!((c.target.as_str(), c.source.as_str()), ("issued at step 3", "issued at step 5"));
        assert!(c.choices.contains(&Choice::Target) && c.choices.contains(&Choice::Source));
        // Unanswered: nothing is merged, and the conflict is listed.
        assert!(p.ops.is_empty() && p.unanswered.contains(&c.id), "{:?}", p.unanswered);
        // It is the only conflict: the revision rows do not raise a second, unnamed one.
        assert_eq!(p.conflicts.len(), 1, "{:?}", p.conflicts.iter().map(|c| &c.what).collect::<Vec<_>>());
        // Answered, the answer decides: the branch's row replaces main's.
        let answers = BTreeMap::from([(c.id.clone(), Choice::Source)]);
        let p = plan(&a, &t, &s, &answers);
        assert!(p.unanswered.is_empty(), "{:?}", p.unanswered);
        assert!(
            p.ops.iter().any(|o| matches!(o, ecad_model::Op::ModifyProject { after, .. }
                if after.revisions.first().and_then(|r| r.description.as_ref()).is_some_and(|d| d.as_str() == "from the branch"))),
            "{:?}",
            p.ops
        );
        let answers = BTreeMap::from([(c.id.clone(), Choice::Target)]);
        assert!(plan(&a, &t, &s, &answers).ops.is_empty(), "keeping main's row changes nothing");
    }

    #[test]
    fn different_revisions_or_a_version_of_the_same_spelling_do_not_clash() {
        let a = Document::default();
        // B on main, C on the branch: two revisions, no clash.
        let (t, s) = (issued(Rev::Letter(2), 3, "b"), issued(Rev::Letter(3), 5, "c"));
        assert!(!plan(&a, &t, &s, &BTreeMap::new()).conflicts.iter().any(|c| c.id.ends_with("/revision.issued")));
        // A user version "B" on the branch beside revision B on main: kinds differ, no clash, and
        // main's points are untouched (version names are branch-local history).
        let mut s = Document::default();
        s.edit_parts(|parts| parts.versions.insert(PointName::version("B").unwrap(), 5));
        let t = issued(Rev::Letter(2), 3, "b");
        let p = plan(&a, &t, &s, &BTreeMap::new());
        assert!(p.conflicts.is_empty(), "{:?}", p.conflicts.iter().map(|c| &c.what).collect::<Vec<_>>());
        assert!(p.ops.iter().all(|o| !matches!(o, ecad_model::Op::NameVersion { .. } | ecad_model::Op::UnnameVersion { .. })));
        // The same version name on both branches is two branch-local points: the merge neither
        // picks between them nor carries one across, so each side keeps its own.
        let (mut t, mut s) = (Document::default(), Document::default());
        t.edit_parts(|parts| parts.versions.insert(PointName::version("B").unwrap(), 3));
        s.edit_parts(|parts| parts.versions.insert(PointName::version("B").unwrap(), 5));
        let p = plan(&a, &t, &s, &BTreeMap::new());
        assert!(p.conflicts.is_empty() && p.ops.is_empty());
    }
}
