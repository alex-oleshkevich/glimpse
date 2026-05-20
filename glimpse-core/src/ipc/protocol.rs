use std::time::{SystemTime, UNIX_EPOCH};

/// A single event emitted by the dispatcher.
#[derive(Debug, Clone)]
pub struct IpcEvent {
    pub name: String,
    pub ts: u64,
    pub fields: Vec<(String, String)>,
}

impl IpcEvent {
    pub fn new(name: impl Into<String>, fields: Vec<(String, String)>) -> Self {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            name: name.into(),
            ts,
            fields,
        }
    }

    /// Encode to wire format: `name key=value key2=value2 ts=<epoch>`
    pub fn encode(&self) -> String {
        let mut parts = vec![self.name.clone()];
        for (k, v) in &self.fields {
            parts.push(format!("{}={}", k, escape(v)));
        }
        parts.push(format!("ts={}", self.ts));
        parts.join(" ")
    }
}

/// Wire format for messages sent from clients to the server.
///
/// `subscribe pattern1 pattern2` — subscribe to event patterns
/// `unsubscribe pattern1 pattern2` — remove subscriptions
/// `<command_name> [key=value ...]` — invoke a shell command
#[derive(Debug)]
pub enum ClientMsg {
    Subscribe(Vec<String>),
    Unsubscribe(Vec<String>),
    Command {
        name: String,
        fields: Vec<(String, String)>,
    },
}

/// Parse a line sent by the client.
pub fn parse_client_line(line: &str) -> Result<ClientMsg, String> {
    let mut tokens = line.split_ascii_whitespace();
    let verb = tokens.next().ok_or("empty line")?;
    match verb {
        "subscribe" => {
            let patterns: Vec<String> = tokens.map(str::to_owned).collect();
            if patterns.is_empty() {
                return Err("subscribe requires at least one pattern".into());
            }
            Ok(ClientMsg::Subscribe(patterns))
        }
        "unsubscribe" => {
            let patterns: Vec<String> = tokens.map(str::to_owned).collect();
            if patterns.is_empty() {
                return Err("unsubscribe requires at least one pattern".into());
            }
            Ok(ClientMsg::Unsubscribe(patterns))
        }
        name => {
            let fields = tokens.map(parse_field).collect::<Result<Vec<_>, _>>()?;
            Ok(ClientMsg::Command {
                name: name.to_owned(),
                fields,
            })
        }
    }
}

/// Parse `key=value` into `(key, value)`.
fn parse_field(token: &str) -> Result<(String, String), String> {
    let (k, v) = token
        .split_once('=')
        .ok_or_else(|| format!("invalid field (no '='): {token}"))?;
    Ok((k.to_owned(), unescape(v)))
}

/// Escape a value for wire encoding. The wire format is newline-delimited and
/// space-separated, so `\`, newline, tab and space must all be encoded or an
/// attacker-influenced field (notification body, window/media title, clipboard
/// preview, SSID, BT device name) could forge or split event lines.
/// Backslash must be replaced first so the escapes we introduce aren't re-escaped.
pub fn escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
        .replace(' ', "\\s")
}

/// Unescape a wire-encoded value.
pub fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('s') => out.push(' '),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some(x) => {
                    out.push('\\');
                    out.push(x);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Returns true when `pattern` matches event `name`.
///
/// Three forms: `*` (any), `bluetooth.*` (prefix), exact name.
pub fn matches_pattern(pattern: &str, name: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix(".*") {
        return name.starts_with(prefix) && name[prefix.len()..].starts_with('.');
    }
    pattern == name
}

/// Encode the hello line sent on connect.
pub fn hello_line() -> String {
    format!("hello version={}", env!("CARGO_PKG_VERSION"))
}

/// Encode an ack line for a command response.
pub fn ack_line(ok: bool, error: Option<&str>) -> String {
    if ok {
        "ack ok=true".to_owned()
    } else {
        format!(
            "ack ok=false error={}",
            escape(error.unwrap_or("unknown error"))
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_wildcard() {
        assert!(matches_pattern("*", "bluetooth.device_connected"));
        assert!(matches_pattern("*", "audio.volume_changed"));
    }

    #[test]
    fn matches_prefix() {
        assert!(matches_pattern("bluetooth.*", "bluetooth.device_connected"));
        assert!(matches_pattern("bluetooth.*", "bluetooth.scanning_started"));
        assert!(!matches_pattern("bluetooth.*", "audio.volume_changed"));
        assert!(!matches_pattern("bluetooth.*", "bluetooth_extra.foo"));
        // namespace prefix alone does not match
        assert!(!matches_pattern("bluetooth.*", "bluetooth"));
    }

    #[test]
    fn matches_exact() {
        assert!(matches_pattern(
            "audio.volume_changed",
            "audio.volume_changed"
        ));
        assert!(!matches_pattern("audio.volume_changed", "audio.muted"));
    }

    #[test]
    fn escape_roundtrip() {
        let s = "hello world\\backslash";
        assert_eq!(unescape(&escape(s)), s);
    }

    #[test]
    fn escape_neutralizes_control_chars_and_roundtrips() {
        // A field value that would otherwise forge/split an event line.
        let s = "evil ts=0\nmpris.track_changed\ttitle=spoofed \\x";
        let e = escape(s);
        assert!(!e.contains('\n'), "newline must not survive encoding: {e}");
        assert!(!e.contains('\t'), "tab must not survive encoding: {e}");
        assert_eq!(unescape(&e), s);
    }

    #[test]
    fn encoded_event_is_single_line_even_with_newline_field() {
        let ev = IpcEvent {
            name: "notification.received".into(),
            ts: 0,
            fields: vec![("body".into(), "line1\nline2".into())],
        };
        assert_eq!(ev.encode().matches('\n').count(), 0);
    }

    #[test]
    fn parse_subscribe() {
        let msg = parse_client_line("subscribe bluetooth.* audio.*").unwrap();
        assert!(matches!(msg, ClientMsg::Subscribe(p) if p == ["bluetooth.*", "audio.*"]));
    }

    #[test]
    fn parse_command() {
        let msg = parse_client_line("open_uri uri=https://example.com").unwrap();
        match msg {
            ClientMsg::Command { name, fields } => {
                assert_eq!(name, "open_uri");
                assert_eq!(fields[0], ("uri".into(), "https://example.com".into()));
            }
            _ => panic!("expected command"),
        }
    }

    #[test]
    fn event_encode() {
        let ev = IpcEvent {
            name: "audio.volume_changed".into(),
            ts: 0,
            fields: vec![("volume".into(), "80".into())],
        };
        let line = ev.encode();
        assert!(line.starts_with("audio.volume_changed volume=80 ts="));
    }
}
