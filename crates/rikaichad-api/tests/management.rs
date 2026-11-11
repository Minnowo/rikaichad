// SPDX-License-Identifier: AGPL-3.0-or-later
//! Management API over a real socket (docs/api.md, milestone 4): uploads
//! and jobs, removal, settings, read-only refusals, concurrent import and
//! lookup, sentence parsing, source.

#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use rikaichad_api::ServeConfig;
use rikaichad_format::Limits;
use rikaichad_store::Store;
use rikaichad_testkit::http::{HttpResponse, post_json, request};
use rikaichad_testkit::{TempDir, fixture};
use serde_json::Value;

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
        rikaichad_api::run(config, |addr| tx.send(addr).unwrap()).unwrap();
    });
    rx.recv().unwrap()
}

fn upload(addr: SocketAddr, zip: &[u8], query: &str) -> HttpResponse {
    request(
        addr,
        "POST",
        &format!("/v1/dictionaries{query}"),
        &[("Content-Type", "application/zip")],
        zip,
    )
}

fn json_request(addr: SocketAddr, method: &str, path: &str, body: &str) -> HttpResponse {
    request(
        addr,
        method,
        path,
        &[("Content-Type", "application/json")],
        body.as_bytes(),
    )
}

/// Polls a job until it is no longer running.
fn wait_job(addr: SocketAddr, id: u64) -> Value {
    let started = Instant::now();
    loop {
        let job = request(addr, "GET", &format!("/v1/jobs/{id}"), &[], b"").json()["job"].clone();
        if job["state"] != "running" {
            return job;
        }
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "job {id} did not finish"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A sequenced dictionary of `rows` generated terms, in banks of 20,000.
fn big_dictionary(title: &str, rows: usize) -> Vec<u8> {
    let mut banks = Vec::new();
    for bank in 0..rows.div_ceil(20_000) {
        let mut text = String::from("[");
        for i in bank * 20_000..((bank + 1) * 20_000).min(rows) {
            if i % 20_000 != 0 {
                text.push(',');
            }
            text.push_str(&format!(
                r#"["語{i}","ご{i}","n","",0,["word number {i}","meaning {}"],{i},""]"#,
                i % 97
            ));
        }
        text.push(']');
        banks.push((format!("term_bank_{}.json", bank + 1), text));
    }
    let files: Vec<(&str, &str)> = banks
        .iter()
        .map(|(n, t)| (n.as_str(), t.as_str()))
        .collect();
    fixture::dictionary(&fixture::index(title), &files)
}

fn titles(addr: SocketAddr) -> Vec<String> {
    request(addr, "GET", "/v1/dictionaries", &[], b"").json()["dictionaries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["title"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn read_only_refuses_every_mutation() {
    let dir = TempDir::new("mgmt-read-only");
    let addr = start(&dir, |c| c.read_only = true);
    let zip = fixture::terms_dictionary("Other");
    let cases: Vec<HttpResponse> = vec![
        upload(addr, &zip, ""),
        request(addr, "DELETE", "/v1/dictionaries/1", &[], b""),
        json_request(addr, "PATCH", "/v1/dictionaries/1", r#"{"enabled":false}"#),
        json_request(addr, "PUT", "/v1/dictionaries/order", r#"{"ids":[1]}"#),
        json_request(addr, "PATCH", "/v1/settings", r#"{"main_dictionary":null}"#),
        request(addr, "DELETE", "/v1/jobs/1", &[], b""),
    ];
    for (i, res) in cases.iter().enumerate() {
        assert_eq!(res.status, 403, "case {i}");
        assert_eq!(res.json()["error"]["code"], "read_only", "case {i}");
    }
    // Reading still works, and nothing changed.
    assert_eq!(titles(addr), ["Fixture"]);
    assert_eq!(request(addr, "GET", "/v1/settings", &[], b"").status, 200);
    assert_eq!(request(addr, "GET", "/v1/source", &[], b"").status, 200);
}

#[test]
fn upload_imports_in_a_job() {
    let dir = TempDir::new("mgmt-upload");
    let addr = start(&dir, |_| {});
    let res = upload(
        addr,
        &fixture::terms_dictionary("Second"),
        "?filename=..%2Fmy%20dict",
    );
    assert_eq!(res.status, 202, "{}", String::from_utf8_lossy(&res.body));
    let job = &res.json()["job"];
    let id = job["id"].as_u64().unwrap();
    assert_eq!(res.header("location").unwrap(), format!("/v1/jobs/{id}"));
    assert_eq!(job["kind"], "import");
    // The client's name, reduced to a base name with .zip.
    assert_eq!(job["source_name"], "my dict.zip");

    let job = wait_job(addr, id);
    assert_eq!(job["state"], "succeeded", "{job}");
    assert_eq!(job["dictionary"]["title"], "Second");
    assert_eq!(
        job["progress"]["files_done"],
        job["progress"]["files_total"]
    );
    assert!(job["finished_at"].is_string());
    assert_eq!(titles(addr), ["Fixture", "Second"]);
    // The upload file is gone.
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.starts_with(".rikaichad-upload"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");

    // Same title again: the job fails with the conflict code.
    let res = upload(addr, &fixture::terms_dictionary("Second"), "");
    let job = wait_job(addr, res.json()["job"]["id"].as_u64().unwrap());
    assert_eq!(job["state"], "failed");
    assert_eq!(job["error"]["code"], "conflict");
    assert_eq!(job["source_name"], "upload.zip");

    // Not a zip.
    let res = upload(addr, b"not a zip", "");
    let job = wait_job(addr, res.json()["job"]["id"].as_u64().unwrap());
    assert_eq!(job["state"], "failed");
    assert_eq!(job["error"]["code"], "invalid_dictionary");

    // A finished job can be forgotten.
    let res = request(addr, "DELETE", &format!("/v1/jobs/{id}"), &[], b"");
    assert_eq!(res.status, 200);
    assert_eq!(
        request(addr, "GET", &format!("/v1/jobs/{id}"), &[], b"").status,
        404
    );
}

#[test]
fn uploads_are_checked_and_capped() {
    let dir = TempDir::new("mgmt-upload-caps");
    let addr = start(&dir, |c| {
        c.max_upload_bytes = 1000;
        c.idle_timeout = Duration::from_secs(1);
    });
    let res = request(
        addr,
        "POST",
        "/v1/dictionaries",
        &[("Content-Type", "application/json")],
        b"{}",
    );
    assert_eq!(res.status, 415);
    let big = vec![0u8; 2000];
    let res = upload(addr, &big, "");
    assert_eq!(res.status, 413);
    assert_eq!(res.json()["error"]["code"], "payload_too_large");
    // Without a declared length (chunked), the bytes are counted.
    let chunk = "x".repeat(600);
    let (reply, _) = rikaichad_testkit::http::raw(
        addr,
        format!(
            "POST /v1/dictionaries HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/zip\r\n\
             Transfer-Encoding: chunked\r\nConnection: close\r\n\r\n258\r\n{chunk}\r\n258\r\n{chunk}\r\n0\r\n\r\n"
        )
        .as_bytes(),
        Duration::from_secs(5),
    );
    let reply = String::from_utf8_lossy(&reply);
    assert!(reply.starts_with("HTTP/1.1 413"), "{reply}");
    // Ids that are not numbers get the JSON envelope.
    let res = request(addr, "DELETE", "/v1/dictionaries/abc", &[], b"");
    assert_eq!(res.status, 400);
    assert_eq!(res.json()["error"]["code"], "bad_request");
    let res = request(addr, "GET", "/v1/jobs/-1", &[], b"");
    assert_eq!(res.status, 400);
    assert_eq!(res.json()["error"]["code"], "bad_request");

    // A stalled upload holds the write slot until it times out; meanwhile
    // other writes are busy.
    let mut stalled = TcpStream::connect(addr).unwrap();
    stalled
        .write_all(
            format!(
                "POST /v1/dictionaries HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/zip\r\n\
                 Content-Length: 900\r\n\r\nPK"
            )
            .as_bytes(),
        )
        .unwrap();
    std::thread::sleep(Duration::from_millis(200));
    let res = upload(addr, b"PK", "");
    assert_eq!(res.status, 409);
    assert_eq!(res.json()["error"]["code"], "busy");
    let res = request(addr, "DELETE", "/v1/dictionaries/1", &[], b"");
    assert_eq!(res.status, 409);
    // Settings writes are busy too, at once rather than after SQLite's
    // busy wait.
    let started = Instant::now();
    let res = json_request(addr, "PATCH", "/v1/settings", r#"{"main_dictionary":null}"#);
    assert_eq!(res.status, 409);
    assert!(started.elapsed() < Duration::from_millis(500));
    let res = json_request(addr, "PATCH", "/v1/dictionaries/1", r#"{"enabled":false}"#);
    assert_eq!(res.status, 409);
    // The stalled upload gets 408 and frees the slot; the partial upload
    // is gone.
    stalled
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut reply = Vec::new();
    let _ = std::io::Read::read_to_end(&mut stalled, &mut reply);
    let reply = String::from_utf8_lossy(&reply);
    assert!(reply.starts_with("HTTP/1.1 408"), "{reply}");
    assert!(reply.contains(r#""code":"timeout""#), "{reply}");
    drop(stalled);
    let res = upload(addr, b"PK", "");
    assert_eq!(res.status, 202);
    wait_job(addr, res.json()["job"]["id"].as_u64().unwrap());
    let leftovers = std::fs::read_dir(dir.path())
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".rikaichad-upload")
        })
        .count();
    assert_eq!(leftovers, 0);
}

#[test]
fn lookups_continue_during_an_import_and_imports_can_be_cancelled() {
    let dir = TempDir::new("mgmt-concurrent");
    let addr = start(&dir, |_| {});
    let res = upload(addr, &big_dictionary("Big", 60_000), "?filename=big.zip");
    assert_eq!(res.status, 202);
    let id = res.json()["job"]["id"].as_u64().unwrap();
    // While it runs, lookups answer from the committed dictionaries.
    let mut answered = 0;
    while request(addr, "GET", &format!("/v1/jobs/{id}"), &[], b"").json()["job"]["state"]
        == "running"
    {
        let res = post_json(
            addr,
            "/v1/terms/lookup",
            r#"{"text":"食べる","mode":"exact"}"#,
        );
        assert_eq!(res.status, 200);
        assert_eq!(res.json()["entries"].as_array().unwrap().len(), 1);
        answered += 1;
    }
    assert!(answered > 0, "the import finished before any lookup");
    assert_eq!(wait_job(addr, id)["state"], "succeeded");
    let res = post_json(
        addr,
        "/v1/terms/lookup",
        r#"{"text":"語123","mode":"exact"}"#,
    );
    assert_eq!(res.json()["entries"][0]["headwords"][0]["reading"], "ご123");

    // Cancel a second import once it has imported a bank: nothing is
    // imported.
    let res = upload(addr, &big_dictionary("Cancelled", 100_000), "");
    let id = res.json()["job"]["id"].as_u64().unwrap();
    let started = Instant::now();
    loop {
        let job = request(addr, "GET", &format!("/v1/jobs/{id}"), &[], b"").json()["job"].clone();
        assert_eq!(
            job["state"], "running",
            "finished before it could be cancelled"
        );
        if job["progress"]["files_done"].as_u64().unwrap() >= 1 {
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(60));
        std::thread::sleep(Duration::from_millis(10));
    }
    let res = request(addr, "DELETE", &format!("/v1/jobs/{id}"), &[], b"");
    assert_eq!(res.status, 202);
    assert_eq!(wait_job(addr, id)["state"], "cancelled");
    assert_eq!(titles(addr), ["Fixture", "Big"]);
}

#[test]
fn remove_patch_order_and_settings() {
    let dir = TempDir::new("mgmt-edit");
    let addr = start(&dir, |_| {});
    let res = upload(addr, &fixture::terms_dictionary("Second"), "");
    let job = wait_job(addr, res.json()["job"]["id"].as_u64().unwrap());
    let second = job["dictionary"]["id"].as_i64().unwrap();

    let res = json_request(
        addr,
        "PATCH",
        &format!("/v1/dictionaries/{second}"),
        r#"{"alias":"Two","enabled":false,"allow_secondary_searches":true}"#,
    );
    assert_eq!(res.status, 200, "{}", String::from_utf8_lossy(&res.body));
    let body = res.json();
    assert_eq!(body["alias"], "Two");
    assert_eq!(body["enabled"], false);
    assert_eq!(body["allow_secondary_searches"], true);
    let res = json_request(
        addr,
        "PATCH",
        &format!("/v1/dictionaries/{second}"),
        r#"{"alias":null}"#,
    );
    assert!(res.json()["alias"].is_null());
    assert_eq!(res.json()["enabled"], false, "unchanged");
    let res = json_request(addr, "PATCH", "/v1/dictionaries/999", r#"{"enabled":true}"#);
    assert_eq!(res.status, 404);
    let res = json_request(
        addr,
        "PATCH",
        &format!("/v1/dictionaries/{second}"),
        r#"{"bogus":1}"#,
    );
    assert_eq!(res.status, 400);

    let res = json_request(
        addr,
        "PUT",
        "/v1/dictionaries/order",
        &format!(r#"{{"ids":[{second}]}}"#),
    );
    assert_eq!(res.status, 200);
    assert_eq!(titles(addr), ["Second", "Fixture"]);
    let res = json_request(addr, "PUT", "/v1/dictionaries/order", r#"{"ids":[1,1]}"#);
    assert_eq!(res.status, 400);
    let res = json_request(addr, "PUT", "/v1/dictionaries/order", r#"{"ids":[999]}"#);
    assert_eq!(res.status, 404);

    let res = request(addr, "GET", "/v1/settings", &[], b"");
    assert_eq!(res.json()["main_dictionary"], "Fixture");
    let res = json_request(
        addr,
        "PATCH",
        "/v1/settings",
        r#"{"sort_frequency_dictionary":"Second","sort_frequency_order":"ascending"}"#,
    );
    assert_eq!(res.status, 200);
    assert_eq!(res.json()["sort_frequency_dictionary"], "Second");
    assert_eq!(res.json()["main_dictionary"], "Fixture", "unchanged");
    let res = json_request(
        addr,
        "PATCH",
        "/v1/settings",
        r#"{"main_dictionary":"Nope"}"#,
    );
    assert_eq!(res.status, 400);
    let res = json_request(
        addr,
        "PATCH",
        "/v1/settings",
        r#"{"sort_frequency_order":"up"}"#,
    );
    assert_eq!(res.status, 400);

    let res = request(
        addr,
        "DELETE",
        &format!("/v1/dictionaries/{second}"),
        &[],
        b"",
    );
    assert_eq!(res.status, 200);
    assert_eq!(res.json()["removed"]["title"], "Second");
    assert_eq!(titles(addr), ["Fixture"]);
    let res = request(
        addr,
        "DELETE",
        &format!("/v1/dictionaries/{second}"),
        &[],
        b"",
    );
    assert_eq!(res.status, 404);
    // Removing the sort-frequency dictionary cleared the setting.
    let res = request(addr, "GET", "/v1/settings", &[], b"");
    assert!(res.json()["sort_frequency_dictionary"].is_null());
}

#[test]
fn text_is_parsed_into_words() {
    let dir = TempDir::new("mgmt-parse");
    let addr = start(&dir, |_| {});
    let res = post_json(addr, "/v1/text/parse", r#"{"text":"ご飯を食べなかった。"}"#);
    assert_eq!(res.status, 200, "{}", String::from_utf8_lossy(&res.body));
    let tokens = res.json()["tokens"].clone();
    let texts: Vec<&str> = tokens
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["text"].as_str().unwrap())
        .collect();
    assert_eq!(texts, ["ご飯", "を", "食べなかった", "。"]);
    assert_eq!(tokens[0]["segments"][1]["reading"], "はん");
    assert_eq!(tokens[2]["segments"][0]["text"], "食");
    assert_eq!(tokens[2]["segments"][0]["reading"], "た");
    assert_eq!(tokens[2]["headwords"][0][0]["term"], "食べる");
    assert_eq!(tokens[1]["headwords"].as_array().unwrap().len(), 0);

    let long = format!(r#"{{"text":"{}"}}"#, "あ".repeat(1001));
    assert_eq!(post_json(addr, "/v1/text/parse", &long).status, 400);
    assert_eq!(
        post_json(addr, "/v1/text/parse", r#"{"text":"x","scan_length":0}"#).status,
        400
    );
}

#[test]
fn allowed_origins_may_use_patch() {
    let dir = TempDir::new("mgmt-cors");
    let addr = start(&dir, |c| {
        c.allowed_origins.push("http://localhost:5173".to_string())
    });
    let res = request(
        addr,
        "OPTIONS",
        "/v1/settings",
        &[
            ("Origin", "http://localhost:5173"),
            ("Access-Control-Request-Method", "PATCH"),
        ],
        b"",
    );
    assert_eq!(res.status, 204);
    let methods = res.header("access-control-allow-methods").unwrap();
    assert!(methods.contains("PATCH"), "{methods}");
}

#[test]
fn source_names_the_build() {
    let dir = TempDir::new("mgmt-source");
    let addr = start(&dir, |c| {
        c.source_url = Some("https://example.org/rikaichad".to_string())
    });
    let body = request(addr, "GET", "/v1/source", &[], b"").json();
    assert_eq!(body["name"], "rikaichad");
    assert_eq!(body["repository"], "https://example.org/rikaichad");
    assert_eq!(body["license"], "AGPL-3.0-or-later AND GPL-3.0-or-later");
    let commit = body["commit"].as_str().unwrap();
    assert!(commit == "unknown" || commit.len() == 40, "{commit}");
}
