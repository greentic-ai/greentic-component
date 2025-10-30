use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use component_store::{ComponentStore, DigestPolicy, VerificationPolicy};

fn spawn_http_server(body: &'static [u8]) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind http listener");
    let addr = listener.local_addr().expect("local addr");

    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buffer = [0u8; 512];
            let _ = stream.read(&mut buffer);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/wasm\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(body);
        }
    });

    format!("http://{}:{}/component.wasm", addr.ip(), addr.port())
}

#[test]
fn fetch_http_component() {
    let cache_dir = tempfile::tempdir().expect("cache dir");
    let payload: &'static [u8] = b"wasm!";
    let url = spawn_http_server(payload);

    let store = ComponentStore::new(cache_dir.path()).expect("store");
    let policy = VerificationPolicy {
        digest: Some(DigestPolicy::sha256(None, false)),
        signature: None,
    };

    let artifact = store
        .fetch_from_str(&url, &policy)
        .expect("fetch from http");
    assert_eq!(artifact.bytes, payload);
    assert!(artifact.verification.digest.is_some());
}
