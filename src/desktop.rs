//! Third source: the usage history the Claude desktop app keeps for itself.
//!
//! `%APPDATA%\Claude\plan-usage-history.json` is a rolling log the app appends
//! to roughly every 15 minutes:
//!
//! ```json
//! {"version":2,"samples":[{"t":1788435667000,"org":"<uuid>","u":{"fh":8,"sd":19}}]}
//! ```
//!
//! `fh` and `sd` are the five-hour and seven-day percentages. There are no
//! reset times here, which is why `Window::resets_at` is optional.
//!
//! The format is undocumented and carries a version number, so every field is
//! treated as optional: anything unexpected means this source stays quiet.

use std::path::PathBuf;

use serde::Deserialize;

use crate::model::{Snapshot, Source, Window};

pub fn history_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("Claude").join("plan-usage-history.json"))
}

#[derive(Debug, Deserialize)]
struct History {
    #[serde(default)]
    samples: Vec<Sample>,
}

#[derive(Debug, Deserialize)]
struct Sample {
    /// Milliseconds since the epoch.
    t: i64,
    #[serde(default)]
    org: Option<String>,
    #[serde(default)]
    u: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    /// Five-hour window, percent.
    #[serde(default)]
    fh: Option<f64>,
    /// Seven-day window, percent.
    #[serde(default)]
    sd: Option<f64>,
}

pub fn read() -> Option<Snapshot> {
    let text = std::fs::read_to_string(history_path()?).ok()?;
    parse(&text)
}

/// The newest sample wins. With several organisations in the file, only the one
/// the newest sample belongs to is considered — mixing percentages from two
/// plans would produce a number that is true of neither.
pub fn parse(text: &str) -> Option<Snapshot> {
    let history: History = serde_json::from_str(text).ok()?;
    let newest = history
        .samples
        .iter()
        .filter(|s| s.u.is_some())
        .max_by_key(|s| s.t)?;
    let org = newest.org.as_deref();
    let sample = history
        .samples
        .iter()
        .filter(|s| s.u.is_some() && s.org.as_deref() == org)
        .max_by_key(|s| s.t)?;

    let usage = sample.u.as_ref()?;
    let observed_at = sample.t / 1000;
    let window = |pct: Option<f64>| {
        pct.map(|used_pct| Window {
            used_pct,
            resets_at: None,
            observed_at,
            source: Source::Desktop,
        })
    };
    let snap = Snapshot {
        five_hour: window(usage.fh),
        seven_day: window(usage.sd),
        context: None,
    };
    // A sample with neither figure is not worth reporting as an observation.
    if snap.five_hour.is_none() && snap.seven_day.is_none() {
        return None;
    }
    Some(snap)
}

/// Every sample the file holds, for the history log to seed itself from.
///
/// Same rule as `parse`: only the organisation of the newest sample, so a file
/// written across two plans does not mix percentages that belong to neither.
pub fn samples(text: &str) -> Vec<crate::history::Sample> {
    let Ok(history) = serde_json::from_str::<History>(text) else {
        return Vec::new();
    };
    let Some(newest) = history.samples.iter().filter(|s| s.u.is_some()).max_by_key(|s| s.t) else {
        return Vec::new();
    };
    let org = newest.org.clone();
    let mut out: Vec<crate::history::Sample> = history
        .samples
        .iter()
        .filter(|s| s.org == org)
        .filter_map(|s| {
            let u = s.u.as_ref()?;
            if u.fh.is_none() && u.sd.is_none() {
                return None;
            }
            Some(crate::history::Sample {
                t: s.t / 1000,
                fh: u.fh,
                sd: u.sd,
            })
        })
        .collect();
    out.sort_by_key(|s| s.t);
    out
}

/// The samples on disk, or nothing if the app has never written any.
pub fn read_samples() -> Vec<crate::history::Sample> {
    let Some(path) = history_path() else {
        return Vec::new();
    };
    match std::fs::read_to_string(path) {
        Ok(text) => samples(&text),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL: &str = r#"{"version":2,"samples":[
        {"t":1788434567000,"org":"org-a","u":{"fh":74,"sd":18}},
        {"t":1788435467000,"org":"org-a","u":{"fh":0,"sd":19}},
        {"t":1788436367000,"org":"org-a","u":{"fh":8,"sd":19}}
    ]}"#;

    #[test]
    fn takes_the_newest_sample() {
        let snap = parse(REAL).unwrap();
        assert_eq!(snap.five_hour.unwrap().used_pct, 8.0);
        assert_eq!(snap.seven_day.unwrap().used_pct, 19.0);
        assert_eq!(snap.five_hour.unwrap().observed_at, 1_788_436_367);
        assert_eq!(snap.five_hour.unwrap().source, Source::Desktop);
        assert!(snap.five_hour.unwrap().resets_at.is_none());
    }

    #[test]
    fn ignores_samples_from_another_organisation() {
        let text = r#"{"version":2,"samples":[
            {"t":1788436367000,"org":"org-a","u":{"fh":8,"sd":19}},
            {"t":1788436000000,"org":"org-b","u":{"fh":90,"sd":90}}
        ]}"#;
        let snap = parse(text).unwrap();
        assert_eq!(snap.five_hour.unwrap().used_pct, 8.0);
    }

    #[test]
    fn a_partial_sample_still_reports_what_it_has() {
        let text = r#"{"version":2,"samples":[{"t":1788436367000,"u":{"sd":19}}]}"#;
        let snap = parse(text).unwrap();
        assert!(snap.five_hour.is_none());
        assert_eq!(snap.seven_day.unwrap().used_pct, 19.0);
    }

    #[test]
    fn nothing_usable_means_silence() {
        assert!(parse("").is_none());
        assert!(parse("not json at all").is_none());
        assert!(parse(r#"{"version":2,"samples":[]}"#).is_none());
        assert!(parse(r#"{"version":2,"samples":[{"t":1,"u":{}}]}"#).is_none());
    }

    #[test]
    fn every_sample_of_the_newest_organisation_is_offered_as_history() {
        let all = samples(REAL);
        assert_eq!(all.len(), 3, "{all:?}");
        assert_eq!(all[0].t, 1_788_434_567, "seconds, not milliseconds");
        assert_eq!(all[0].fh, Some(74.0));
        assert_eq!(all[2].fh, Some(8.0), "sorted oldest first");

        let mixed = r#"{"version":2,"samples":[
            {"t":1788436367000,"org":"org-a","u":{"fh":8,"sd":19}},
            {"t":1788436000000,"org":"org-b","u":{"fh":90,"sd":90}}
        ]}"#;
        let all = samples(mixed);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].fh, Some(8.0));

        assert!(samples("not json").is_empty());
        assert!(samples(r#"{"samples":[]}"#).is_empty());
    }

    #[test]
    fn an_unknown_version_with_a_familiar_shape_is_still_read() {
        // The app has already moved from one version to another once; a bumped
        // number alone is no reason to go blind.
        let text = r#"{"version":9,"samples":[{"t":1788436367000,"u":{"fh":5,"sd":6},"x":true}]}"#;
        assert_eq!(parse(text).unwrap().five_hour.unwrap().used_pct, 5.0);
    }
}
