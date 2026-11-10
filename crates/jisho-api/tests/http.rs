// SPDX-License-Identifier: AGPL-3.0-or-later
//! The HTTP API over a real socket: status codes, error envelopes, limits,
//! Host/Origin checks (docs/api.md).

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use std::net::SocketAddr;
use std::sync::mpsc;
use std::time::Duration;

use jisho_api::ServeConfig;
use jisho_format::Limits;
use jisho_store::Store;
use jisho_testkit::http::{post_json, raw, request};
use jisho_testkit::{TempDir, fixture};

/// Starts a server on an ephemeral loopback port with a store holding the
/// fixture dictionary. The server thread lives until the test process ends.
fn start(dir: &TempDir, adjust: impl FnOnce(&mut ServeConfig)) -> SocketAddr {
    let store_path = dir.path().join("store.sqlite");
    let zip = dir.write("a.zip", &fixture::terms_dictionary("Fixture"));
    Store::open(&store_path)
        .unwrap()
        .import(&zip, &Limits::default(), &mut |_| {})
        .unwrap();
    let mut config = ServeConfig::new(store_path, "127.0.0.1:0".parse().unwrap());
    adjust(&mut config);
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        jisho_api::run(config, |addr| tx.send(addr).unwrap()).unwrap();
    });
    rx.recv().unwrap()
}

#[test]
fn health_reports_version_and_mode() {
    let dir = TempDir::new("http-health");
    let addr = start(&dir, |c| c.read_only = true);
    let res = request(addr, "GET", "/v1/health", &[], b"");
    assert_eq!(res.status, 200);
    assert!(
        res.header("content-type")
            .unwrap()
            .starts_with("application/json")
    );
    let body = res.json();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["read_only"], true);
}

#[test]
fn exact_lookup_returns_entries_and_lengths() {
    let dir = TempDir::new("http-lookup");
    let addr = start(&dir, |_| {});
    let res = post_json(
        addr,
        "/v1/terms/lookup",
        r#"{"text":"食う","mode":"exact"}"#,
    );
    assert_eq!(res.status, 200);
    let body = res.json();
    let entry = &body["entries"][0];
    let definition = &entry["definitions"][0];
    assert_eq!(definition["dictionary"]["title"], "Fixture");
    assert_eq!(entry["headwords"][0]["term"], "食う");
    assert_eq!(entry["headwords"][0]["reading"], "くう");
    assert_eq!(definition["glossary"][0]["type"], "structured-content");
    assert_eq!(
        definition["glossary_text"][0],
        "- to eat (rough)\n- to 喰(く)う"
    );
    assert_eq!(body["matched"]["text"], "食う");
    assert_eq!(body["matched"]["length"]["utf16"], 2);
    assert_eq!(body["matched"]["length"]["chars"], 2);
    assert_eq!(body["truncated"], false);
}

#[test]
fn scan_deinflects_and_reports_the_chain() {
    let dir = TempDir::new("http-scan");
    let addr = start(&dir, |_| {});
    let res = post_json(addr, "/v1/terms/lookup", r#"{"text":"食べられなかったよ"}"#);
    assert_eq!(res.status, 200);
    let body = res.json();
    let entry = &body["entries"][0];
    assert_eq!(entry["headwords"][0]["term"], "食べる");
    let source = &entry["headwords"][0]["sources"][0];
    assert_eq!(source["original_text"], "食べられなかった");
    assert_eq!(source["deinflected_text"], "食べる");
    let rules: Vec<&str> = entry["inflections"][0]["rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    // Yomitan order: the inflection nearest the dictionary form first.
    assert_eq!(rules, ["potential or passive", "negative", "-た"]);
    assert_eq!(body["matched"]["text"], "食べられなかった");
    assert_eq!(body["matched"]["length"]["utf16"], 8);
}

#[test]
fn lookup_limit_truncates() {
    let dir = TempDir::new("http-limit");
    let addr = start(&dir, |_| {});
    // A second dictionary imported while the server runs; the server's
    // connections see it once the import commits.
    let second = dir.write("b.zip", &fixture::terms_dictionary("Second"));
    Store::open(&dir.path().join("store.sqlite"))
        .unwrap()
        .import(&second, &Limits::default(), &mut |_| {})
        .unwrap();
    let res = post_json(
        addr,
        "/v1/terms/lookup",
        r#"{"text":"食べる","mode":"exact","limit":1}"#,
    );
    let body = res.json();
    assert_eq!(body["entries"].as_array().unwrap().len(), 1);
    assert_eq!(body["truncated"], true);
}

#[test]
fn lookup_request_validation() {
    let dir = TempDir::new("http-validation");
    let addr = start(&dir, |c| c.max_json_body = 1024);
    let cases = [
        (
            r#"{"text":"x","mode":"exact","extra":1}"#,
            400,
            "bad_request",
        ),
        (r#"{"text":"","mode":"exact"}"#, 400, "bad_request"),
        (
            r#"{"text":"x","mode":"exact","limit":0}"#,
            400,
            "bad_request",
        ),
        (r#"{"text":"x","mode":"fuzzy"}"#, 400, "bad_request"),
        ("not json", 400, "bad_request"),
    ];
    for (body, status, code) in cases {
        let res = post_json(addr, "/v1/terms/lookup", body);
        assert_eq!(res.status, status, "{body}");
        assert_eq!(res.json()["error"]["code"], code, "{body}");
    }
    let long = format!(r#"{{"text":"{}","mode":"exact"}}"#, "あ".repeat(1001));
    let res = post_json(addr, "/v1/terms/lookup", &long);
    assert_eq!(res.status, 413, "over the 1 KiB test body cap");
    let res = request(
        addr,
        "POST",
        "/v1/terms/lookup",
        &[("Content-Type", "text/plain")],
        br#"{"text":"x","mode":"exact"}"#,
    );
    assert_eq!(res.status, 415);
    assert_eq!(res.json()["error"]["code"], "unsupported_media_type");
}

#[test]
fn text_over_1000_chars_is_rejected() {
    let dir = TempDir::new("http-text-cap");
    let addr = start(&dir, |_| {});
    let long = format!(r#"{{"text":"{}","mode":"exact"}}"#, "あ".repeat(1001));
    let res = post_json(addr, "/v1/terms/lookup", &long);
    assert_eq!(res.status, 400);
    assert_eq!(res.json()["error"]["code"], "bad_request");
}

#[test]
fn unknown_paths_and_methods_use_the_envelope() {
    let dir = TempDir::new("http-404");
    let addr = start(&dir, |_| {});
    let res = request(addr, "GET", "/v1/nope", &[], b"");
    assert_eq!(res.status, 404);
    assert_eq!(res.json()["error"]["code"], "not_found");
    let res = request(addr, "GET", "/v1/terms/lookup", &[], b"");
    assert_eq!(res.status, 405);
    assert_eq!(res.json()["error"]["code"], "method_not_allowed");
}

#[test]
fn dictionaries_are_listed() {
    let dir = TempDir::new("http-dicts");
    let addr = start(&dir, |_| {});
    let res = request(addr, "GET", "/v1/dictionaries", &[], b"");
    assert_eq!(res.status, 200);
    let body = res.json();
    let dict = &body["dictionaries"][0];
    assert_eq!(dict["title"], "Fixture");
    assert_eq!(dict["counts"]["terms"], 4);
    assert_eq!(dict["has_styles"], true);
    assert!(dict["imported_at"].as_str().unwrap().ends_with('Z'));
}

#[test]
fn rebinding_host_is_rejected_on_loopback() {
    let dir = TempDir::new("http-host");
    let addr = start(&dir, |_| {});
    let res = request(
        addr,
        "GET",
        "/v1/health",
        &[("Host", "evil.example:47613")],
        b"",
    );
    assert_eq!(res.status, 403);
    assert_eq!(res.json()["error"]["code"], "forbidden_host");
    let res = request(addr, "GET", "/v1/health", &[("Host", "localhost")], b"");
    assert_eq!(res.status, 200);
}

#[test]
fn origins_are_checked_and_allowed_ones_get_cors() {
    let dir = TempDir::new("http-origin");
    let addr = start(&dir, |c| {
        c.allowed_origins = vec!["http://localhost:5173".to_string()];
    });
    let res = request(
        addr,
        "GET",
        "/v1/health",
        &[("Origin", "https://evil.example")],
        b"",
    );
    assert_eq!(res.status, 403);
    assert_eq!(res.json()["error"]["code"], "forbidden_origin");

    let res = request(
        addr,
        "GET",
        "/v1/health",
        &[("Origin", "http://localhost:5173")],
        b"",
    );
    assert_eq!(res.status, 200);
    assert_eq!(
        res.header("access-control-allow-origin"),
        Some("http://localhost:5173")
    );

    let res = request(
        addr,
        "OPTIONS",
        "/v1/terms/lookup",
        &[
            ("Origin", "http://localhost:5173"),
            ("Access-Control-Request-Method", "POST"),
        ],
        b"",
    );
    assert_eq!(res.status, 204);
    assert!(
        res.header("access-control-allow-methods")
            .unwrap()
            .contains("POST")
    );
}

#[test]
fn slow_request_body_times_out() {
    let dir = TempDir::new("http-slow-body");
    let addr = start(&dir, |c| c.request_timeout = Duration::from_secs(1));
    // Promise 100 bytes, send 10, then wait: the request must fail with
    // 503 at the time limit instead of holding the connection.
    let head = format!(
        "POST /v1/terms/lookup HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\n\
         Content-Length: 100\r\nConnection: close\r\n\r\n{{\"text\":\"a"
    );
    let (out, took) = raw(addr, head.as_bytes(), Duration::from_secs(5));
    let text = String::from_utf8_lossy(&out);
    assert!(text.starts_with("HTTP/1.1 503"), "{text}");
    assert!(text.contains("\"timeout\""), "{text}");
    assert!(took < Duration::from_secs(4), "took {took:?}");
}

#[test]
fn silent_connection_is_closed_after_idle_timeout() {
    let dir = TempDir::new("http-idle");
    let addr = start(&dir, |c| c.idle_timeout = Duration::from_secs(1));
    let (out, took) = raw(addr, b"", Duration::from_secs(5));
    assert!(out.is_empty(), "{}", String::from_utf8_lossy(&out));
    assert!(
        took < Duration::from_secs(4),
        "connection stayed open {took:?}"
    );
}

#[test]
fn non_text_origin_is_rejected() {
    let dir = TempDir::new("http-bad-origin");
    let addr = start(&dir, |_| {});
    let mut req = format!("GET /v1/health HTTP/1.1\r\nHost: {addr}\r\nOrigin: ").into_bytes();
    req.extend_from_slice(b"http://\xff\xfe\r\nConnection: close\r\n\r\n");
    let (out, _) = raw(addr, &req, Duration::from_secs(5));
    let text = String::from_utf8_lossy(&out);
    assert!(text.starts_with("HTTP/1.1 403"), "{text}");
    assert!(text.contains("forbidden_origin"), "{text}");
}

#[test]
fn oversized_content_length_is_rejected_before_reading() {
    let dir = TempDir::new("http-big-cl");
    let addr = start(&dir, |c| c.max_json_body = 1024);
    let head = format!(
        "POST /v1/terms/lookup HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\n\
         Content-Length: 99999999\r\nConnection: close\r\n\r\n"
    );
    let (out, _) = raw(addr, head.as_bytes(), Duration::from_secs(5));
    let text = String::from_utf8_lossy(&out);
    assert!(text.starts_with("HTTP/1.1 413"), "{text}");
}
