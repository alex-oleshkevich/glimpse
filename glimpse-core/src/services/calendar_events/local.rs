use std::{fs, path::Path};

use anyhow::{Context, anyhow};

use crate::{CalendarSourceConfig, CalendarSourceType};

use super::{ical, source::SourceSnapshot};

pub fn load_directory_source(config: &CalendarSourceConfig) -> anyhow::Result<SourceSnapshot> {
    if config.source_type != CalendarSourceType::Directory {
        return Err(anyhow!(
            "calendar source {} is not a directory source",
            config.id
        ));
    }
    let path = file_uri_path(&config.uri)?;
    let mut snapshot = SourceSnapshot::default();

    for entry in fs::read_dir(path)
        .with_context(|| format!("failed to read calendar directory {}", path.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("ics") {
            continue;
        }
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "failed to read iCalendar file");
                continue;
            }
        };
        let mut file_snapshot = match ical::parse_ical_source(config, &content) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "failed to parse iCalendar file");
                continue;
            }
        };
        if snapshot.source.source_id.is_empty() {
            snapshot.source = file_snapshot.source.clone();
        }
        snapshot.events.append(&mut file_snapshot.events);
    }

    snapshot.events.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| a.end.cmp(&b.end))
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.event_id.cmp(&b.event_id))
    });
    Ok(snapshot)
}

fn file_uri_path(uri: &str) -> anyhow::Result<&Path> {
    let path = uri
        .strip_prefix("file://")
        .ok_or_else(|| anyhow!("directory calendar sources must use file:// URIs"))?;
    Ok(Path::new(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CalendarSourceConfig, CalendarSourceType};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_dir() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("glimpse-calendar-dir-{stamp}"));
        fs::create_dir_all(&dir).expect("test directory should be created");
        dir
    }

    fn directory_config(uri: String) -> CalendarSourceConfig {
        CalendarSourceConfig {
            id: "local".into(),
            source_type: CalendarSourceType::Directory,
            name: Some("Local".into()),
            uri,
            poll_interval: None,
            color: None,
        }
    }

    #[test]
    fn load_directory_source_merges_ics_files_and_ignores_other_files() {
        let dir = temp_dir();
        fs::write(
            dir.join("work.ics"),
            "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:work-1\nSUMMARY:Planning\nDTSTART:20260526T080000Z\nDTEND:20260526T090000Z\nEND:VEVENT\nEND:VCALENDAR\n",
        )
        .expect("work calendar should be written");
        fs::write(
            dir.join("home.ics"),
            "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:home-1\nSUMMARY:Dinner\nDTSTART:20260526T180000Z\nDTEND:20260526T190000Z\nEND:VEVENT\nEND:VCALENDAR\n",
        )
        .expect("home calendar should be written");
        fs::write(dir.join("notes.txt"), "not a calendar").expect("ignored file should be written");
        let config = directory_config(format!("file://{}", dir.display()));

        let snapshot = load_directory_source(&config).expect("directory source should load");

        assert_eq!(snapshot.events.len(), 2);
        assert_eq!(snapshot.events[0].event_id, "work-1");
        assert_eq!(snapshot.events[1].event_id, "home-1");
    }

    #[test]
    fn load_directory_source_keeps_valid_ics_files_when_one_file_is_invalid() {
        let dir = temp_dir();
        fs::write(
            dir.join("valid.ics"),
            "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:valid-1\nSUMMARY:Planning\nDTSTART:20260526T080000Z\nDTEND:20260526T090000Z\nEND:VEVENT\nEND:VCALENDAR\n",
        )
        .expect("valid calendar should be written");
        fs::write(
            dir.join("broken.ics"),
            "BEGIN:VCALENDAR\nBEGIN:VEVENT\nSUMMARY:Missing UID\nDTSTART:20260526T080000Z\nEND:VEVENT\nEND:VCALENDAR\n",
        )
        .expect("broken calendar should be written");
        let config = directory_config(format!("file://{}", dir.display()));

        let snapshot = load_directory_source(&config).expect("directory source should load");

        assert_eq!(snapshot.events.len(), 1);
        assert_eq!(snapshot.events[0].event_id, "valid-1");
    }
}
