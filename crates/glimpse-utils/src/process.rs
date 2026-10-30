const MAX_APP_ID: usize = 255;

struct Stat {
    parent: u32,
    terminal: bool,
}

fn parse_stat(stat: &str) -> Option<Stat> {
    let (_, rest) = stat.rsplit_once(')')?;
    let mut fields = rest.split_whitespace();
    let parent = fields.nth(1)?.parse().ok()?;
    let terminal = fields.nth(2)?.parse::<i64>().ok()? != 0;
    Some(Stat { parent, terminal })
}

fn stat(pid: u32) -> Option<Stat> {
    parse_stat(&std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)
}

pub fn parent(pid: u32) -> Option<u32> {
    stat(pid).map(|stat| stat.parent)
}

pub fn has_terminal(pid: u32) -> bool {
    stat(pid).is_some_and(|stat| stat.terminal)
}

pub fn exe_name(pid: u32) -> Option<String> {
    let executable = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?;
    executable.file_name()?.to_str().map(str::to_owned)
}

pub fn unit_app_ids(pid: u32) -> Vec<String> {
    std::fs::read_to_string(format!("/proc/{pid}/cgroup"))
        .map(|cgroup| app_ids_of_cgroup(&cgroup))
        .unwrap_or_default()
}

fn unit_name(unit: &str) -> Option<(&str, bool)> {
    let name = unit.strip_prefix("app-")?;
    if let Some(name) = name.strip_suffix(".scope") {
        return Some((name, true));
    }
    let name = name.strip_suffix(".service")?;
    Some((name.split('@').next().unwrap_or(name), false))
}

pub fn app_ids_of_cgroup(cgroup: &str) -> Vec<String> {
    let Some(path) = cgroup.lines().find_map(|line| line.strip_prefix("0::")) else {
        return Vec::new();
    };
    let Some((name, scope)) = path.split('/').rev().find_map(unit_name) else {
        return Vec::new();
    };
    let mut segments: Vec<&str> = name.split('-').collect();
    if scope
        && segments.len() >= 2
        && segments.last().is_some_and(|last| {
            !last.is_empty() && last.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    {
        segments.pop();
    }
    let mut ids: Vec<String> = Vec::new();
    for index in 0..segments.len() {
        let Some(candidate) = unescape(&segments[index..].join("-")) else {
            continue;
        };
        if valid(&candidate) && !ids.contains(&candidate) {
            ids.push(candidate);
        }
    }
    ids
}

fn unescape(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            if bytes.get(index + 1) != Some(&b'x') {
                return None;
            }
            let hex = text.get(index + 2..index + 4)?;
            let byte = u8::from_str_radix(hex, 16).ok()?;
            if !byte.is_ascii() {
                return None;
            }
            out.push(byte);
            index += 4;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn valid(candidate: &str) -> bool {
    !candidate.is_empty()
        && candidate.len() <= MAX_APP_ID
        && candidate
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        && !candidate.starts_with(['.', '-'])
}

#[cfg(test)]
mod tests;
