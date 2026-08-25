/// `*` matches one segment, `**` matches one or more trailing segments, anything else is literal.
pub fn matches(pattern: &str, topic: &str) -> bool {
    let mut segments = topic.split('.');
    let mut expected = pattern.split('.').peekable();

    while let Some(part) = expected.next() {
        if part == "**" {
            return expected.peek().is_none() && segments.next().is_some();
        }
        match segments.next() {
            Some(segment) if part == "*" || part == segment => {}
            _ => return false,
        }
    }

    segments.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn patterns_match_what_they_say() {
        let cases = [
            ("audio.volume", "audio.volume", true),
            ("audio.volume", "audio.mute", false),
            ("audio.volume", "audio", false),
            ("audio.*", "audio.volume", true),
            ("audio.*", "audio.sink.default", false),
            ("tray.**", "tray.item.nextcloud", true),
            ("tray.**", "tray.item.nextcloud.menu", true),
            ("tray.**", "tray", false),
            ("**", "anything.at.all", true),
        ];

        for (pattern, topic, expected) in cases {
            assert_eq!(matches(pattern, topic), expected, "{pattern} vs {topic}");
        }
    }
}
