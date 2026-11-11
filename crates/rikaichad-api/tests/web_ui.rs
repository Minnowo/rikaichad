// SPDX-License-Identifier: AGPL-3.0-or-later
//! What the web UI needs from the server: its files with security headers,
//! its own origin accepted, dictionary styles, and headword furigana.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use std::net::SocketAddr;
use std::sync::mpsc;

use rikaichad_api::ServeConfig;
use rikaichad_format::Limits;
use rikaichad_store::Store;
use rikaichad_testkit::http::{post_json, request};
use rikaichad_testkit::{TempDir, fixture};

#[test]
fn index_and_assets_are_served_with_csp() {
    let dir = TempDir::new("ui-assets");
    let addr = start(&dir);

    let res = request(addr, "GET", "/", &[], b"");
    assert_eq!(res.status, 200);
    assert_eq!(res.header("content-type"), Some("text/html; charset=utf-8"));
    assert_eq!(
        res.header("content-security-policy"),
        Some(
            "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; \
             img-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"
        )
    );
    assert_eq!(res.header("x-content-type-options"), Some("nosniff"));
    let html = String::from_utf8(res.body.clone()).unwrap();
    assert!(html.contains("/ui/main.js"), "{html}");

    let res = request(addr, "GET", "/ui/main.js", &[], b"");
    assert_eq!(res.status, 200);
    assert_eq!(
        res.header("content-type"),
        Some("text/javascript; charset=utf-8")
    );
    assert!(res.header("content-security-policy").is_some());

    let res = request(addr, "GET", "/ui/app.css", &[], b"");
    assert_eq!(res.status, 200);
    assert_eq!(res.header("content-type"), Some("text/css; charset=utf-8"));
}

#[test]
fn unknown_ui_files_are_404() {
    let dir = TempDir::new("ui-404");
    let addr = start(&dir);
    for path in ["/ui/missing.js", "/ui/..%2Fbuild.rs", "/ui/"] {
        let res = request(addr, "GET", path, &[], b"");
        assert_eq!(res.status, 404, "{path}");
        assert_eq!(res.json()["error"]["code"], "not_found", "{path}");
    }
}

#[test]
fn own_origin_is_allowed_and_others_are_not() {
    let dir = TempDir::new("ui-origin");
    let addr = start(&dir);
    let body = r#"{"text":"食う","mode":"exact"}"#.as_bytes();
    let own = format!("http://{addr}");
    let res = request(
        addr,
        "POST",
        "/v1/terms/lookup",
        &[("Origin", &own), ("Content-Type", "application/json")],
        body,
    );
    assert_eq!(res.status, 200);
    // Same origin needs no CORS headers.
    assert_eq!(res.header("access-control-allow-origin"), None);

    // Behind a TLS proxy that keeps the Host header; names are compared
    // without regard to case.
    let host = format!("localhost:{}", addr.port());
    for origin in [
        format!("https://{host}"),
        format!("http://LOCALHOST:{}", addr.port()),
    ] {
        let res = request(
            addr,
            "POST",
            "/v1/terms/lookup",
            &[
                ("Host", &host),
                ("Origin", &origin),
                ("Content-Type", "application/json"),
            ],
            body,
        );
        assert_eq!(res.status, 200, "{origin}");
    }

    // A page on another port of the same host is another origin.
    let other = format!("http://127.0.0.1:{}", addr.port().wrapping_add(1));
    for origin in [other.as_str(), "http://evil.example", "null"] {
        let res = request(
            addr,
            "POST",
            "/v1/terms/lookup",
            &[("Origin", origin), ("Content-Type", "application/json")],
            body,
        );
        assert_eq!(res.status, 403, "{origin}");
        assert_eq!(res.json()["error"]["code"], "forbidden_origin");
    }

    // The Host check still comes first: a rebinding attack sends a
    // matching Origin and Host for its own name.
    let res = request(
        addr,
        "POST",
        "/v1/terms/lookup",
        &[
            ("Host", "evil.example"),
            ("Origin", "http://evil.example"),
            ("Content-Type", "application/json"),
        ],
        body,
    );
    assert_eq!(res.status, 403);
    assert_eq!(res.json()["error"]["code"], "forbidden_host");
}

#[test]
fn rebinding_on_a_wildcard_bind_is_not_same_origin() {
    // Bound to all interfaces without allowed_hosts, any Host is accepted,
    // so a DNS name in Host must not make its matching Origin trusted: a
    // rebinding page sends exactly that pair. IP literals stay trusted.
    let dir = TempDir::new("ui-wildcard");
    let addr = start_with(&dir, "0.0.0.0:0", &[]);
    let body = br#"{"text":"x","mode":"exact"}"#;
    let attack = request(
        addr,
        "POST",
        "/v1/terms/lookup",
        &[
            ("Host", "evil.example:47613"),
            ("Origin", "http://evil.example:47613"),
            ("Content-Type", "application/json"),
        ],
        body,
    );
    assert_eq!(attack.status, 403);
    assert_eq!(attack.json()["error"]["code"], "forbidden_origin");

    let own = format!("http://{addr}");
    let res = request(
        addr,
        "POST",
        "/v1/terms/lookup",
        &[("Origin", &own), ("Content-Type", "application/json")],
        body,
    );
    assert_eq!(res.status, 200);

    // A name the user listed in allowed_hosts is trusted.
    let dir = TempDir::new("ui-wildcard-named");
    let addr = start_with(&dir, "0.0.0.0:0", &["rikaichad.lan"]);
    let res = request(
        addr,
        "POST",
        "/v1/terms/lookup",
        &[
            ("Host", "rikaichad.lan:47613"),
            ("Origin", "http://rikaichad.lan:47613"),
            ("Content-Type", "application/json"),
        ],
        body,
    );
    assert_eq!(res.status, 200);
}

#[test]
fn ui_and_styles_are_served_read_only() {
    let dir = TempDir::new("ui-read-only");
    let store_path = dir.path().join("store.sqlite");
    let zip = dir.write("a.zip", &fixture::terms_dictionary("Fixture"));
    Store::open(&store_path)
        .unwrap()
        .import(&zip, &Limits::default(), &mut |_| {})
        .unwrap();
    let mut config = ServeConfig::new(store_path, "127.0.0.1:0".parse().unwrap());
    config.read_only = true;
    let addr = serve(config);
    for path in ["/", "/ui/main.js", "/v1/dictionaries/1/styles"] {
        assert_eq!(request(addr, "GET", path, &[], b"").status, 200, "{path}");
    }
}

#[test]
fn dictionary_styles_are_served_as_css() {
    let dir = TempDir::new("ui-styles");
    let addr = start(&dir);
    let res = request(addr, "GET", "/v1/dictionaries/1/styles", &[], b"");
    assert_eq!(res.status, 200);
    assert_eq!(res.header("content-type"), Some("text/css; charset=utf-8"));
    assert_eq!(res.body, b".x { color: red; }");

    let res = request(addr, "GET", "/v1/dictionaries/99/styles", &[], b"");
    assert_eq!(res.status, 404);
    assert_eq!(res.json()["error"]["code"], "not_found");
    let res = request(addr, "GET", "/v1/dictionaries/x/styles", &[], b"");
    assert_eq!(res.status, 400);
}

#[test]
fn dictionary_without_styles_is_404() {
    let dir = TempDir::new("ui-no-styles");
    let store_path = dir.path().join("store.sqlite");
    let zip = dir.write(
        "a.zip",
        &fixture::dictionary(&fixture::index("Plain"), &[("term_bank_1.json", "[]")]),
    );
    Store::open(&store_path)
        .unwrap()
        .import(&zip, &Limits::default(), &mut |_| {})
        .unwrap();
    let addr = serve(ServeConfig::new(store_path, "127.0.0.1:0".parse().unwrap()));
    let res = request(addr, "GET", "/v1/dictionaries/1/styles", &[], b"");
    assert_eq!(res.status, 404);
}

#[test]
fn headwords_carry_furigana() {
    let dir = TempDir::new("ui-furigana");
    let addr = start(&dir);
    let res = post_json(
        addr,
        "/v1/terms/lookup",
        r#"{"text":"食う","mode":"exact"}"#,
    );
    assert_eq!(res.status, 200);
    let furigana = &res.json()["entries"][0]["headwords"][0]["furigana"];
    assert_eq!(
        furigana,
        &serde_json::json!([{"text": "食", "reading": "く"}, {"text": "う", "reading": ""}])
    );
}

/// Starts a server on the fixture dictionary (which has styles.css).
fn start(dir: &TempDir) -> SocketAddr {
    start_with(dir, "127.0.0.1:0", &[])
}

/// Like [`start`] bound to `bind`; returns a loopback address to connect
/// to even when bound to all interfaces.
fn start_with(dir: &TempDir, bind: &str, allowed_hosts: &[&str]) -> SocketAddr {
    let store_path = dir.path().join("store.sqlite");
    let zip = dir.write("a.zip", &fixture::terms_dictionary("Fixture"));
    Store::open(&store_path)
        .unwrap()
        .import(&zip, &Limits::default(), &mut |_| {})
        .unwrap();
    let mut config = ServeConfig::new(store_path, bind.parse().unwrap());
    config.allowed_hosts = allowed_hosts.iter().map(|h| h.to_string()).collect();
    let addr = serve(config);
    SocketAddr::from(([127, 0, 0, 1], addr.port()))
}

fn serve(config: ServeConfig) -> SocketAddr {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        rikaichad_api::run(config, |addr| tx.send(addr).unwrap()).unwrap();
    });
    rx.recv().unwrap()
}
