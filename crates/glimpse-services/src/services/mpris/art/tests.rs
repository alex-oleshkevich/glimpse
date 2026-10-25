use super::*;

#[test]
fn a_file_url_becomes_the_path_it_names() {
    assert_eq!(
        classify(Some("file:///music/cover.png")),
        Some(Art::Local("/music/cover.png".to_owned()))
    );
}

#[test]
fn a_file_url_is_percent_decoded() {
    assert_eq!(
        classify(Some("file:///music/My%20Album/cover.png")),
        Some(Art::Local("/music/My Album/cover.png".to_owned()))
    );
}

#[test]
fn a_bare_absolute_path_is_taken_as_one() {
    assert_eq!(
        classify(Some("/music/cover.png")),
        Some(Art::Local("/music/cover.png".to_owned()))
    );
}

#[test]
fn http_and_https_are_the_only_schemes_that_are_fetched() {
    assert_eq!(
        classify(Some("https://example.test/a.png")),
        Some(Art::Remote("https://example.test/a.png".to_owned()))
    );
    assert!(matches!(
        classify(Some("http://example.test/a.png")),
        Some(Art::Remote(_))
    ));
}

#[test]
fn any_other_scheme_is_refused() {
    for refused in [
        "javascript:alert(1)",
        "data:image/png;base64,AAAA",
        "ftp://example.test/a.png",
        "mailto:someone@example.test",
    ] {
        assert_eq!(classify(Some(refused)), None, "{refused} was accepted");
    }
}

#[test]
fn a_file_url_is_taken_at_its_word() {
    assert_eq!(
        classify(Some("file:///../../etc/shadow")),
        Some(Art::Local("/etc/shadow".to_owned()))
    );
}

#[test]
fn nothing_at_all_is_nothing() {
    assert_eq!(classify(None), None);
    assert_eq!(classify(Some("")), None);
    assert_eq!(classify(Some("   ")), None);
    assert_eq!(classify(Some("not a url")), None);
}

#[tokio::test]
#[ignore = "sets XDG_RUNTIME_DIR, so it runs alone"]
async fn a_fetch_saves_a_small_body_and_refuses_one_past_the_cap() {
    let runtime = std::env::temp_dir().join(format!("glimpse-art-{}", std::process::id()));
    std::fs::create_dir_all(&runtime).unwrap();
    unsafe { std::env::set_var("XDG_RUNTIME_DIR", &runtime) };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 512];
                let Ok(n) = sock.read(&mut buf).await else {
                    return;
                };
                let request = String::from_utf8_lossy(&buf[..n]);
                let path = request.split_whitespace().nth(1).unwrap_or("");
                if path == "/small" {
                    let _ = sock
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\n",
                        )
                        .await;
                    let _ = sock.write_all(b"ok!\n").await;
                } else if path == "/big" {
                    let _ = sock
                        .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n")
                        .await;
                    let _ = sock.write_all(&vec![b'x'; 2048]).await;
                }
            });
        }
    });

    let client = reqwest::Client::builder().build().unwrap();
    let small = format!("http://{addr}/small");
    match fetch(client.clone(), small, 1).await {
        Event::Art {
            path: Some(path), ..
        } => assert_eq!(tokio::fs::read(&path).await.unwrap(), b"ok!\n"),
        Event::Art { path: None, .. } => panic!("a 4-byte body was not saved"),
        _ => panic!("fetch returned something other than artwork"),
    }

    let big = format!("http://{addr}/big");
    match fetch(client, big.clone(), 1).await {
        Event::Art { path: None, .. } => {}
        Event::Art {
            path: Some(path), ..
        } => panic!("stored {path} past the cap"),
        _ => panic!("fetch returned something other than artwork"),
    }
    assert!(!cached(&big).unwrap().exists());
    let _ = std::fs::remove_dir_all(&runtime);
}

#[test]
fn a_chunk_past_the_cap_is_refused_and_the_buffer_stays_inside_it() {
    let mut body = vec![1, 2, 3];
    assert!(!append_capped(&mut body, &[4, 5], 4));
    assert_eq!(body, vec![1, 2, 3]);
    assert!(append_capped(&mut body, &[4], 4));
    assert_eq!(body, vec![1, 2, 3, 4]);
}

#[test]
fn one_url_always_hashes_to_one_name() {
    assert_eq!(
        digest("https://example.test/a.png"),
        digest("https://example.test/a.png")
    );
    assert_ne!(
        digest("https://example.test/a.png"),
        digest("https://example.test/b.png")
    );
    assert_eq!(digest("https://example.test/a.png").len(), 16);
}
