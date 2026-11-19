//! Compressed responses: every API call asks for gzip (and brotli), and a
//! compressed answer is read as if it hadn't been. Cloudflare compresses a
//! reading page about five-fold when asked (528 KB → 107 KB gzip, 81 KB
//! brotli). A local `wrangler dev` answers `gzip, br` with gzip, so the
//! real-studio e2e reads every response gzipped; here the request header and
//! both decoders are checked on the wire, with a one-shot local server.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;

use serde_json::{Value, json};

use super::Api;
use super::auth::Credential;

/// Serve one response with `content-encoding: <encoding>` and `body`
/// (already encoded) on 127.0.0.1; the request's head comes back on the
/// channel.
fn serve_once(encoding: &'static str, body: Vec<u8>) -> (String, mpsc::Receiver<String>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        if let Ok((mut s, _)) = l.accept() {
            let mut buf = [0u8; 4096];
            let mut got = Vec::new();
            while !got.windows(4).any(|w| w == b"\r\n\r\n") {
                match s.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => got.extend_from_slice(&buf[..n]),
                }
            }
            let _ = tx.send(String::from_utf8_lossy(&got).to_ascii_lowercase());
            let _ = write!(
                s,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-encoding: {encoding}\r\n\
                 content-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            let _ = s.write_all(&body);
        }
    });
    (format!("http://127.0.0.1:{port}"), rx)
}

/// A reading page of `n` long imported rows, as the studio serves it.
fn page(n: usize) -> Value {
    let rows: Vec<Value> = (0..n)
        .map(|i| {
            let md = format!("Post {i}. ").repeat(600);
            json!({
                "subscriptionId": "S1", "subscriptionTitle": "Ada",
                "origin": "https://ada.blyg.example.com/", "remoteId": format!("R{i}"),
                "kind": "fragment", "withdrawn": false, "l0": false, "version": 1,
                "readVersion": null, "created": "2030-01-01T00:00:00Z",
                "updated": "2030-01-01T00:00:00Z", "observedAt": "2030-01-01T00:00:05Z",
                "contentMd": md, "contentHtml": format!("<p>{md}</p>"),
                "author": {"name": "Ada", "url": null}, "page": null, "sourceUrl": null,
                "pinnedVersionRetained": null, "thumb": null, "hoppers": [],
                "transclusions": null, "stubOf": null, "forkedFrom": null, "generated": null
            })
        })
        .collect();
    json!({"items": rows, "page": {"limit": 100, "nextCursor": null}})
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(bytes).unwrap();
    e.finish().unwrap()
}

#[test]
fn api_calls_ask_for_compression_and_read_a_gzipped_page() {
    let raw = serde_json::to_vec(&page(100)).unwrap();
    let body = gzip(&raw);
    assert!(
        body.len() * 5 < raw.len(),
        "{} vs {}",
        body.len(),
        raw.len()
    );
    let (url, head) = serve_once("gzip", body);
    let api = Api::new(&url, Credential::Token("t".into()));
    let got = api.reading(100, None).unwrap().unwrap();
    assert_eq!(got.items.len(), 100);
    assert_eq!(got.items[42].remote_id, "R42");
    assert!(got.items[42].content_md.starts_with("Post 42. Post 42. "));
    assert_eq!(got.items[42].content_md.len(), "Post 42. ".len() * 600);
    let head = head.recv().unwrap();
    let ae = head
        .lines()
        .find_map(|l| l.strip_prefix("accept-encoding:"))
        .unwrap_or_else(|| panic!("no accept-encoding in {head}"));
    assert!(ae.contains("gzip"), "{ae}");
    assert!(ae.contains("br"), "{ae}");
}

/// An empty reading page whose `nextCursor` is `"brotli-ok."` 500 times,
/// compressed (to 73 bytes) with the `brotli` command line tool, as
/// Burrow carries only the decoder.
const BROTLI_PAGE: &[u8] = include_bytes!("../../tests/fixtures/empty-page.json.br");

#[test]
fn a_brotli_page_reads_too() {
    let (url, _head) = serve_once("br", BROTLI_PAGE.to_vec());
    let api = Api::new(&url, Credential::Token("t".into()));
    let got = api.reading(100, None).unwrap().unwrap();
    assert!(got.items.is_empty());
    assert_eq!(got.next, Some("brotli-ok.".repeat(500)));
}
