use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

const FDO_SIGNALS: [&str; 4] = [
    "owner_changed",
    "name_owner_changed",
    "properties_changed",
    "interfaces_added",
];

const RAW_STREAMS: [&str; 3] = [
    "receive_all_signals(",
    "MessageStream::for_match_rule",
    "MessageStream::from",
];

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn sources() -> Vec<PathBuf> {
    let mut all = Vec::new();
    let Ok(crates) = fs::read_dir(workspace().join("crates")) else {
        return all;
    };
    for krate in crates.flatten() {
        rust_files(&krate.path().join("src"), &mut all);
    }
    all.sort();
    all
}

fn is_test_or_cli(path: &Path) -> bool {
    path.components().any(|part| {
        let Component::Normal(part) = part else {
            return false;
        };
        let part = part.to_string_lossy();
        part == "tests" || part == "tests.rs" || part == "testing" || part == "glimpsectl"
    })
}

fn signal_names(source: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut lines = source.lines().map(str::trim);
    while let Some(line) = lines.next() {
        if line != "#[zbus(signal)]" {
            continue;
        }
        for next in lines.by_ref() {
            if next.starts_with("#[") || next.is_empty() {
                continue;
            }
            let declaration = next
                .trim_start_matches("pub ")
                .trim_start_matches("async ")
                .strip_prefix("fn ");
            if let Some(name) = declaration.and_then(|rest| rest.split(['(', '<']).next()) {
                names.insert(name.to_owned());
            }
            break;
        }
    }
    names
}

fn is_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + word.len()..].chars().next();
        !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
            && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

fn queued(source: &str, start: usize, end: usize) -> bool {
    let mut depth = 0i32;
    for (at, c) in source[..start].char_indices().rev() {
        match c {
            ')' => depth += 1,
            '(' if depth == 0 => {
                let head = source[..at].trim_end();
                let name = head
                    .rsplit(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .next()
                    .unwrap_or_default();
                if name == "wake" || name == "events" {
                    return true;
                }
            }
            '(' => depth -= 1,
            ';' | '{' | '}' if depth == 0 => break,
            _ => {}
        }
    }

    let mut depth = 0i32;
    let mut tail_end = source.len();
    for (at, c) in source[end..].char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ';' | '{' | '}' if depth <= 0 => {
                tail_end = end + at;
                break;
            }
            _ => {}
        }
        if depth < 0 {
            tail_end = end + at;
            break;
        }
    }
    let tail = &source[end..tail_end];
    is_word(tail, "wake") || is_word(tail, "events")
}

fn call_end(source: &str, from: usize) -> usize {
    let Some(open) = source[from..].find('(').map(|at| from + at) else {
        return from;
    };
    let mut depth = 0i32;
    for (at, c) in source[open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return open + at + 1;
                }
            }
            _ => {}
        }
    }
    source.len()
}

fn offenders(source: &str, signals: &BTreeSet<String>) -> Vec<usize> {
    let mut hits = Vec::new();
    for (at, _) in source.match_indices("receive_") {
        let rest = &source[at..];
        let ident: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !rest[ident.len()..].starts_with('(') {
            continue;
        }
        let name = ident["receive_".len()..]
            .strip_suffix("_with_args")
            .unwrap_or(&ident["receive_".len()..]);
        if signals.contains(name) || ident == "receive_all_signals" {
            hits.push((at, call_end(source, at)));
        }
    }
    for raw in RAW_STREAMS {
        if raw.ends_with('(') {
            continue;
        }
        for (at, _) in source.match_indices(raw) {
            hits.push((at, call_end(source, at)));
        }
    }
    hits.sort();
    hits.dedup();
    hits.into_iter()
        .filter(|(start, end)| !queued(source, *start, *end))
        .map(|(start, _)| source[..start].matches('\n').count() + 1)
        .collect()
}

fn known_signals() -> BTreeSet<String> {
    let mut names: BTreeSet<String> = FDO_SIGNALS.iter().map(|name| (*name).to_owned()).collect();
    let mut all = Vec::new();
    rust_files(&workspace().join("crates"), &mut all);
    for path in all {
        if let Ok(source) = fs::read_to_string(&path) {
            names.extend(signal_names(&source));
        }
    }
    names
}

#[test]
fn every_signal_stream_goes_through_wake_or_events() {
    let signals = known_signals();
    let root = workspace();
    let mut failures = Vec::new();
    for path in sources() {
        let relative = path.strip_prefix(&root).unwrap().to_path_buf();
        if is_test_or_cli(&relative) {
            continue;
        }
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        for line in offenders(&source, &signals) {
            failures.push(format!("{}:{line}", relative.display()));
        }
    }
    assert!(
        failures.is_empty(),
        "signal streams not passed into glimpse_dbus::wake or glimpse_dbus::events:\n{}",
        failures.join("\n")
    );
}

#[test]
fn the_scanner_tells_wrapped_streams_from_bare_ones() {
    let signals: BTreeSet<String> = ["new_icon", "name_owner_changed"]
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    let good = [
        "let a = glimpse_dbus::wake(item.receive_new_icon().await?);",
        "if let Ok(s) = item.receive_new_icon().await.map(glimpse_dbus::wake) {\n}",
        "let Ok(s) = dbus.receive_name_owner_changed_with_args(&[(0, n)]).await.map(events) else {\n};",
        "let s = events(MessageStream::for_match_rule(rule, c, None).await.ok()?.filter_map(f));",
        "let s = proxy.receive_all_signals().await.map(|s| glimpse_dbus::events(s.filter_map(f)));",
        "let p = item.receive_status_changed().await;",
        "let v = other.receive_unrelated().await;",
    ];
    for snippet in good {
        assert!(
            offenders(snippet, &signals).is_empty(),
            "flagged a wrapped stream: {snippet}"
        );
    }
    let bad = [
        "let s = item.receive_new_icon().await?;",
        "if let Ok(s) = item.receive_new_icon().await {\n    wake(s);\n}",
        "let s = dbus.receive_name_owner_changed().await.unwrap();\nlet w = wake(x);",
        "let s = MessageStream::for_match_rule(rule, c, None).await?;",
        "let s = MessageStream::from(&connection);",
        "let s = proxy.receive_all_signals().await?;",
    ];
    for snippet in bad {
        assert_eq!(
            offenders(snippet, &signals).len(),
            1,
            "missed a bare stream: {snippet}"
        );
    }
}

#[test]
fn signal_names_come_from_the_zbus_attribute() {
    let names = signal_names(
        "#[zbus(signal)]\nasync fn one(a: u8);\n\n#[zbus(signal)]\n#[allow(x)]\nfn two();\nfn three();\n",
    );
    assert_eq!(names.iter().cloned().collect::<Vec<_>>(), ["one", "two"]);
}
