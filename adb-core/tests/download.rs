//! The download path end to end, against a tiny local server: no internet needed, and the hostile cases
//! (redirects elsewhere, huge or cut-off files) are exercised for real.
#![cfg(unix)]
use adb_core::platform_tools::{self, Progress};
use std::{
    io::{Cursor, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{atomic::{AtomicU64, Ordering}, Arc},
    thread,
};

fn serve(handler: impl Fn(&str, &mut TcpStream) + Send + 'static + Clone) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let h = handler.clone();
            thread::spawn(move || {
                let mut buf = [0u8; 2048];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                h(&path, &mut s);
            });
        }
    });
    format!("http://127.0.0.1:{port}")
}

fn reply(s: &mut TcpStream, status: &str, headers: &str, body: &[u8]) {
    let _ = write!(s, "HTTP/1.1 {status}\r\n{headers}Connection: close\r\n\r\n");
    let _ = s.write_all(body);
}

fn fake_platform_tools_zip() -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    {
        let mut w = zip::ZipWriter::new(&mut buf);
        let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).unix_permissions(0o755);
        w.start_file("platform-tools/adb", o).unwrap();
        w.write_all(b"#!/bin/sh\necho 'Android Debug Bridge version 1.0.41'\necho 'Version 34.0.4-10411341'\n").unwrap();
        w.finish().unwrap();
    }
    buf.into_inner()
}

fn no_progress() -> Progress {
    Arc::new(|_, _| {})
}

const LOCAL: &[&str] = &["127.0.0.1"];

#[test]
fn downloads_with_progress() {
    let body = vec![7u8; 300_000];
    let b2 = body.clone();
    let base = serve(move |_, s| reply(s, "200 OK", &format!("Content-Length: {}\r\n", b2.len()), &b2));
    let seen = Arc::new(AtomicU64::new(0));
    let s2 = seen.clone();
    let progress: Progress = Arc::new(move |got, total| {
        assert_eq!(total, Some(300_000));
        s2.store(got, Ordering::SeqCst);
    });
    let data = platform_tools::download_blocking(&format!("{base}/ok.zip"), LOCAL, true, 1_000_000, &progress).unwrap();
    assert_eq!(data, body);
    assert_eq!(seen.load(Ordering::SeqCst), 300_000, "progress reached the end");
}

#[test]
fn refuses_files_over_the_cap_declared_or_not() {
    let base = serve(|path, s| {
        if path == "/declared" {
            reply(s, "200 OK", "Content-Length: 5000000\r\n", b"x")
        } else {
            // No Content-Length: the server just keeps sending.
            reply(s, "200 OK", "", &vec![1u8; 200_000])
        }
    });
    let e = platform_tools::download_blocking(&format!("{base}/declared"), LOCAL, true, 100_000, &no_progress()).unwrap_err();
    assert!(e.contains("larger than expected"), "{e}");
    let e = platform_tools::download_blocking(&format!("{base}/stream"), LOCAL, true, 100_000, &no_progress()).unwrap_err();
    assert!(e.contains("larger than expected"), "{e}");
}

#[test]
fn refuses_redirects_to_other_hosts_and_loops() {
    let base = serve(|path, s| match path {
        "/away" => reply(s, "302 Found", "Location: http://evil.example/payload.zip\r\nContent-Length: 0\r\n", b""),
        "/https-downgrade" => reply(s, "302 Found", "Location: https://evil.example/x\r\nContent-Length: 0\r\n", b""),
        _ => reply(s, "302 Found", "Location: /loop\r\nContent-Length: 0\r\n", b""),
    });
    for p in ["/away", "/https-downgrade"] {
        let e = platform_tools::download_blocking(&format!("{base}{p}"), LOCAL, true, 1000, &no_progress()).unwrap_err();
        assert!(e.contains("not on the list"), "{p}: {e}");
    }
    let e = platform_tools::download_blocking(&format!("{base}/loop"), LOCAL, true, 1000, &no_progress()).unwrap_err();
    assert!(e.contains("too many redirects"), "{e}");
}

#[test]
fn reports_errors_and_cut_off_downloads() {
    let base = serve(|path, s| match path {
        "/missing" => reply(s, "404 Not Found", "Content-Length: 0\r\n", b""),
        _ => reply(s, "200 OK", "Content-Length: 1000\r\n", b"only a little"),
    });
    assert!(platform_tools::download_blocking(&format!("{base}/missing"), LOCAL, true, 1000, &no_progress()).unwrap_err().contains("404"));
    assert!(platform_tools::download_blocking(&format!("{base}/cut"), LOCAL, true, 5000, &no_progress()).is_err(), "a short body is not accepted");
    assert!(platform_tools::download_blocking("http://127.0.0.1:1/x", LOCAL, true, 1000, &no_progress()).unwrap_err().contains("could not connect"));
    assert!(platform_tools::download_blocking("https://dl.google.com/x", &["other.example"], false, 1000, &no_progress()).unwrap_err().contains("not on the list"));
}

#[tokio::test]
async fn installs_and_proves_the_downloaded_adb_runs() {
    let zip = fake_platform_tools_zip();
    let z2 = zip.clone();
    let base = serve(move |_, s| reply(s, "200 OK", &format!("Content-Length: {}\r\n", z2.len()), &z2));
    let dir = std::env::temp_dir().join(format!("adbm-install-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let adb = platform_tools::install(&dir, &format!("{base}/platform-tools.zip"), LOCAL_STATIC, true, no_progress()).await.unwrap();
    assert_eq!(adb, dir.join("platform-tools").join("adb"));
    let out = adb_core::AdbRunner::new(&adb).run(&["version"]).await.unwrap();
    assert!(out.contains("1.0.41"));
}

#[tokio::test]
async fn a_download_that_is_not_adb_is_rejected_and_nothing_is_left_behind() {
    // A valid zip whose adb does not behave like adb.
    let mut buf = Cursor::new(Vec::new());
    {
        let mut w = zip::ZipWriter::new(&mut buf);
        let o = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
        w.start_file("platform-tools/adb", o).unwrap();
        w.write_all(b"#!/bin/sh\necho hello\n").unwrap();
        w.finish().unwrap();
    }
    let zip = buf.into_inner();
    let base = serve(move |_, s| reply(s, "200 OK", &format!("Content-Length: {}\r\n", zip.len()), &zip));
    let dir = std::env::temp_dir().join(format!("adbm-install-bad-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let e = platform_tools::install(&dir, &format!("{base}/x.zip"), LOCAL_STATIC, true, no_progress()).await.unwrap_err();
    assert!(e.contains("did not answer like adb"), "{e}");
}

const LOCAL_STATIC: &[&str] = &["127.0.0.1"];
