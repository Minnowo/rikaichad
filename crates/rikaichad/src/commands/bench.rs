// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rchad bench`: measure lookup latency against the current store.
//!
//! For every start position in every sentence, the next `--max-length`
//! characters are looked up the way the API does it: a Yomitan-style scan
//! (all prefixes, text variants, deinflections). Timed per scan: finding
//! and ranking entries without glossaries, and the complete lookup with
//! glossaries for the first `--limit` entries.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use lexopt::prelude::*;
use rikaichad_api::{LookupMode, LookupRequest, run_lookup};
use rikaichad_lookup::{EnabledDictionaries, FindTermsOptions, ResultMode, find_terms};

use super::Context;
use crate::output::{CliError, out};
use crate::peak_memory::peak_rss_bytes;

pub const USAGE: &str = "\
Usage: rchad bench [--sentences FILE] [--max-length N] [--limit N] [--result-mode MODE]

Measures scan latency on the current store. From every start position of
every line in FILE (default bench/sentences.txt), the next --max-length
characters (default 16) are looked up like POST /v1/terms/lookup does:
timed once for finding, grouping and ranking entries with their tags and
metadata, once for the full lookup with glossaries of the first --limit
entries (default 32, Yomitan's maxResults). --result-mode is as for
`rchad lookup` (default group). Run it on a machine with nothing else busy.
";

const DEFAULT_SENTENCES: &str = "bench/sentences.txt";
const DEFAULT_MAX_LENGTH: usize = 16;
const DEFAULT_LIMIT: usize = rikaichad_api::DEFAULT_LIMIT;

pub fn run(parser: &mut lexopt::Parser, ctx: &Context) -> Result<ExitCode, CliError> {
    let mut sentences = PathBuf::from(DEFAULT_SENTENCES);
    let mut max_length = DEFAULT_MAX_LENGTH;
    let mut limit = DEFAULT_LIMIT;
    let mut result_mode = ResultMode::default();
    while let Some(arg) = parser.next()? {
        match arg {
            Long("result-mode") => {
                let value = parser.value()?.string()?;
                result_mode = ResultMode::parse(&value)
                    .ok_or_else(|| CliError::new(format!("unknown result mode {value:?}")))?;
            }
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

    let dictionaries_enabled = EnabledDictionaries::from_store(&store)?;
    let options = FindTermsOptions {
        mode: result_mode,
        limit: Some(limit),
        ..FindTermsOptions::default()
    };
    let request = |text: &str| LookupRequest {
        text: text.to_string(),
        mode: LookupMode::Scan,
        result_mode,
        limit,
        primary_reading: String::new(),
        scan_length: max_length,
        max_bytes: usize::MAX,
    };

    // Warm the page cache so every pass sees the same state.
    for scan in &scans {
        run_lookup(&store, &request(scan))?;
    }

    let mut rank_only = Vec::with_capacity(scans.len());
    let mut matched = 0usize;
    for scan in &scans {
        let start = Instant::now();
        matched += find_terms(&store, scan, &dictionaries_enabled, &options)?
            .entries
            .len();
        rank_only.push(start.elapsed());
    }

    let mut full_lookup = Vec::with_capacity(scans.len());
    for scan in &scans {
        let start = Instant::now();
        run_lookup(&store, &request(scan))?;
        full_lookup.push(start.elapsed());
    }

    let mut report = String::new();
    report.push_str(&format!(
        "store: {} ({}), {} dictionaries, {} terms\n",
        ctx.store_path.display(),
        megabytes(store_bytes),
        dictionaries.len(),
        terms
    ));
    report.push_str(&format!(
        "input: {} lines, {} scans of up to {max_length} chars, {matched} entries found\n",
        text.lines().filter(|l| !l.trim().is_empty()).count(),
        scans.len()
    ));
    report.push_str(&format!("result mode: {}\n", result_mode.as_str()));
    report.push_str(&summary("scan, find, group and rank", &mut rank_only));
    report.push_str(&summary(
        &format!("scan, full lookup with glossaries of first {limit}"),
        &mut full_lookup,
    ));
    if let Some(peak) = peak_rss_bytes() {
        report.push_str(&format!("peak memory: {}\n", megabytes(peak)));
    }
    out(&report)?;
    Ok(ExitCode::SUCCESS)
}

/// One scan text per start position: the next `max_length` characters,
/// skipping starts at whitespace or punctuation.
fn build_scans(text: &str, max_length: usize) -> Vec<String> {
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
            let end = start.saturating_add(max_length).min(chars.len());
            scans.push(chars.get(start..end).unwrap_or_default().iter().collect());
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
    fn scans_start_at_each_non_punctuation_character() {
        assert_eq!(build_scans("ab、c\n", 2), ["ab", "b、", "c"]);
    }

    #[test]
    fn huge_max_length_does_not_overflow() {
        assert_eq!(build_scans("ab", usize::MAX).len(), 2);
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
