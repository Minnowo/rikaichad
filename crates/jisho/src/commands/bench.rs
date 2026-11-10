// SPDX-License-Identifier: AGPL-3.0-or-later
//! `jisho bench`: measure lookup latency against the current store.
//!
//! Simulates what a Yomitan-style scan asks the store: for every start
//! position in every sentence, all prefixes of the next `--max-length`
//! characters are candidate strings. Deinflection (milestone 2) adds more
//! candidates per prefix, so these numbers are a lower bound for a scan.
//! Timed per scan: matching all candidates in one batched query (what
//! ranking needs), and matching plus loading glossaries for the first
//! `--limit` hits (what a lookup response needs).

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use lexopt::prelude::*;

use super::Context;
use crate::output::{CliError, out};
use crate::peak_memory::peak_rss_bytes;

pub const USAGE: &str = "\
Usage: jisho bench [--sentences FILE] [--max-length N] [--limit N]

Measures lookup latency on the current store. Every prefix (up to
--max-length characters, default 16) at every start position of every line
in FILE (default bench/sentences.txt) is a candidate. Each scan is timed
matching all candidates in one query, and matching plus loading the
glossaries of the first --limit hits (default 100). Run it on a machine
with nothing else busy.
";

const DEFAULT_SENTENCES: &str = "bench/sentences.txt";
const DEFAULT_MAX_LENGTH: usize = 16;
const DEFAULT_LIMIT: usize = 100;

pub fn run(parser: &mut lexopt::Parser, ctx: &Context) -> Result<ExitCode, CliError> {
    let mut sentences = PathBuf::from(DEFAULT_SENTENCES);
    let mut max_length = DEFAULT_MAX_LENGTH;
    let mut limit = DEFAULT_LIMIT;
    while let Some(arg) = parser.next()? {
        match arg {
            Long("sentences") => sentences = PathBuf::from(parser.value()?),
            Long("max-length") => {
                max_length = parser.value()?.parse()?;
                if max_length == 0 {
                    return Err(CliError::new("--max-length must be at least 1"));
                }
            }
            Long("limit") => limit = parser.value()?.parse()?,
            Short('h') | Long("help") => {
                out(USAGE)?;
                return Ok(ExitCode::SUCCESS);
            }
            _ => return Err(arg.unexpected().into()),
        }
    }
    let text = std::fs::read_to_string(&sentences)
        .map_err(|e| CliError::new(format!("cannot read {}: {e}", sentences.display())))?;
    let scans = build_scans(&text, max_length);
    if scans.is_empty() {
        return Err(CliError::new(format!(
            "{} has no text",
            sentences.display()
        )));
    }
    let store = super::open_store(ctx)?;
    let dictionaries = store.dictionaries()?;
    let terms: u64 = dictionaries.iter().map(|d| d.counts.terms).sum();
    let store_bytes = file_size(&ctx.store_path);

    // Warm the page cache so every pass sees the same state.
    for scan in &scans {
        let mut hits = store.find_terms(scan)?;
        hits.truncate(limit);
        store.with_glossaries(hits)?;
    }

    let mut match_only = Vec::with_capacity(scans.len());
    let mut matched = 0usize;
    for scan in &scans {
        let start = Instant::now();
        matched += store.find_terms(scan)?.len();
        match_only.push(start.elapsed());
    }

    let mut with_glossaries = Vec::with_capacity(scans.len());
    for scan in &scans {
        let start = Instant::now();
        let mut hits = store.find_terms(scan)?;
        hits.truncate(limit);
        store.with_glossaries(hits)?;
        with_glossaries.push(start.elapsed());
    }

    let probes: usize = scans.iter().map(Vec::len).sum();
    let mut report = String::new();
    report.push_str(&format!(
        "store: {} ({}), {} dictionaries, {} terms\n",
        ctx.store_path.display(),
        megabytes(store_bytes),
        dictionaries.len(),
        terms
    ));
    report.push_str(&format!(
        "input: {} lines, {} scans of up to {max_length} chars, {probes} candidates, {matched} rows matched\n",
        text.lines().filter(|l| !l.trim().is_empty()).count(),
        scans.len()
    ));
    report.push_str(&summary("scan, match only", &mut match_only));
    report.push_str(&summary(
        &format!("scan, match + glossaries of first {limit}"),
        &mut with_glossaries,
    ));
    if let Some(peak) = peak_rss_bytes() {
        report.push_str(&format!("peak memory: {}\n", megabytes(peak)));
    }
    report.push_str("query plan:\n");
    for line in store.explain_find_terms(3)? {
        report.push_str(&format!("  {line}\n"));
    }
    out(&report)?;
    Ok(ExitCode::SUCCESS)
}

/// One scan per start position: all prefixes of the next `max_length`
/// characters, skipping starts at whitespace or punctuation.
fn build_scans(text: &str, max_length: usize) -> Vec<Vec<String>> {
    let mut scans = Vec::new();
    for line in text.lines() {
        let chars: Vec<char> = line.trim().chars().collect();
        for start in 0..chars.len() {
            let Some(first) = chars.get(start) else {
                continue;
            };
            if first.is_whitespace() || is_punctuation(*first) {
                continue;
            }
            let end = (start + max_length).min(chars.len());
            let window = chars.get(start..end).unwrap_or_default();
            let scan: Vec<String> = (1..=window.len())
                .map(|n| window.iter().take(n).collect())
                .collect();
            scans.push(scan);
        }
    }
    scans
}

fn is_punctuation(c: char) -> bool {
    c.is_ascii_punctuation()
        || matches!(
            c,
            '\u{3001}' | '\u{3002}' | '\u{ff0c}' | '\u{ff01}' | '\u{ff1f}'
        )
}

/// "label: n=.. p50=.. p90=.. p99=.. max=.." with millisecond values.
fn summary(label: &str, samples: &mut [Duration]) -> String {
    samples.sort_unstable();
    let pick = |q: f64| -> Duration {
        if samples.is_empty() {
            return Duration::ZERO;
        }
        let rank = ((q * samples.len() as f64).ceil() as usize).clamp(1, samples.len());
        samples.get(rank - 1).copied().unwrap_or_default()
    };
    format!(
        "{label}: n={} p50={} p90={} p99={} max={}\n",
        samples.len(),
        ms(pick(0.50)),
        ms(pick(0.90)),
        ms(pick(0.99)),
        ms(pick(1.0))
    )
}

fn ms(d: Duration) -> String {
    format!("{:.3}ms", d.as_secs_f64() * 1000.0)
}

fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

/// Size of the store file plus its WAL, if present.
fn file_size(path: &std::path::Path) -> u64 {
    let main = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    main + std::fs::metadata(PathBuf::from(wal))
        .map(|m| m.len())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_are_prefixes_from_each_non_punctuation_start() {
        let scans = build_scans("ab、c\n", 2);
        assert_eq!(
            scans,
            vec![
                vec!["a".to_string(), "ab".to_string()],
                vec!["b".to_string(), "b、".to_string()],
                vec!["c".to_string()],
            ]
        );
    }

    #[test]
    fn summary_picks_nearest_rank_percentiles() {
        let mut samples: Vec<Duration> = (1..=100).map(Duration::from_millis).collect();
        let text = summary("x", &mut samples);
        assert!(text.contains("p50=50.000ms"), "{text}");
        assert!(text.contains("p99=99.000ms"), "{text}");
        assert!(text.contains("max=100.000ms"), "{text}");
    }
}
