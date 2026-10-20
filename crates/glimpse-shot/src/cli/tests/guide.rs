use super::*;

#[test]
fn prime_prints_the_embedded_guide_without_a_session() {
    let mut output = Vec::new();
    prime::print(&mut output).expect("prime succeeds");
    assert_eq!(output, prime::GUIDE.as_bytes());
    assert!(matches!(
        parse(&["prime"]).expect("parses").command,
        Some(Command::Prime)
    ));
}

#[test]
fn prime_names_every_wire_kind_limit_color_and_exit_code() {
    let samples = [
        AnnotationV1::Arrow {
            from: [0, 0],
            to: [1, 1],
            color: PaletteColor::Red,
            stroke: 2,
        },
        AnnotationV1::Line {
            from: [0, 0],
            to: [1, 1],
            color: PaletteColor::Red,
            stroke: 2,
        },
        AnnotationV1::Freehand {
            points: vec![[0, 0], [1, 1]],
            color: PaletteColor::Red,
            stroke: 2,
        },
        AnnotationV1::Highlighter {
            points: vec![[0, 0], [1, 1]],
            color: PaletteColor::Red,
            stroke: 2,
        },
        AnnotationV1::Rect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            color: PaletteColor::Red,
            stroke: 2,
            filled: false,
        },
        AnnotationV1::Oval {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            color: PaletteColor::Red,
            stroke: 2,
        },
        AnnotationV1::Number {
            x: 0,
            y: 0,
            color: PaletteColor::Red,
            stroke: 2,
            n: 1,
        },
        AnnotationV1::Text {
            x: 0,
            y: 0,
            text: String::new(),
            color: PaletteColor::Red,
            stroke: 2,
        },
        AnnotationV1::Blur {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        },
        AnnotationV1::Pixelate {
            points: vec![[0, 0], [1, 1]],
            width: 32,
        },
    ];
    let count = samples.len();
    let mut kinds = std::collections::HashSet::new();
    for sample in samples {
        match &sample {
            AnnotationV1::Arrow { .. }
            | AnnotationV1::Line { .. }
            | AnnotationV1::Freehand { .. }
            | AnnotationV1::Highlighter { .. }
            | AnnotationV1::Rect { .. }
            | AnnotationV1::Oval { .. }
            | AnnotationV1::Number { .. }
            | AnnotationV1::Text { .. }
            | AnnotationV1::Blur { .. }
            | AnnotationV1::Pixelate { .. } => {}
        }
        let value = serde_json::to_value(sample).expect("serializes");
        let kind = value["kind"].as_str().expect("kind");
        assert!(
            prime::GUIDE.contains(&format!("`{kind}`")),
            "missing {kind}"
        );
        kinds.insert(kind.to_owned());
    }
    assert_eq!(kinds.len(), count);
    for limit in [MAX_MARKS, MAX_PATH_POINTS, MAX_TEXT_CHARS] {
        assert!(prime::GUIDE.contains(&limit.to_string()), "missing {limit}");
    }
    for color in ["red", "yellow", "green", "blue", "white"] {
        assert!(
            prime::GUIDE.contains(&format!("`{color}`")),
            "missing {color}"
        );
    }
    for code in [
        Exit::Ok,
        Exit::Failed,
        Exit::Usage,
        Exit::Config,
        Exit::Cancelled,
        Exit::NotFound,
        Exit::TimedOut,
    ] {
        match code {
            Exit::Ok
            | Exit::Failed
            | Exit::Usage
            | Exit::Config
            | Exit::Cancelled
            | Exit::NotFound
            | Exit::TimedOut => {}
        }
        assert!(
            prime::GUIDE.contains(&format!("| {} |", code as u8)),
            "missing exit code {}",
            code as u8
        );
    }
}

#[test]
fn prime_annotation_json_examples_parse_and_validate() {
    let mut count = 0;
    for fence in prime::GUIDE.split("```json").skip(1) {
        let example = fence.split("```").next().expect("closed fence").trim();
        if example.starts_with('[') {
            let annotations: Vec<AnnotationV1> =
                serde_json::from_str(example).expect("annotation JSON");
            wire::marks(&annotations).expect("valid annotations");
            count += 1;
        }
    }
    assert!(count > 0, "no annotation JSON example");

    let heredoc = prime::GUIDE
        .split("<<'EOF'\n")
        .nth(1)
        .expect("annotation heredoc")
        .split("\nEOF")
        .next()
        .expect("heredoc end");
    let annotations: Vec<AnnotationV1> = serde_json::from_str(heredoc).expect("heredoc JSON");
    wire::marks(&annotations).expect("valid heredoc annotations");
}

#[test]
fn short_and_long_help_point_to_prime_without_a_field_list() {
    for args in [
        &["-h"][..],
        &["--help"][..],
        &["capture", "-h"][..],
        &["capture", "--help"][..],
        &["annotate", "-h"][..],
        &["annotate", "--help"][..],
    ] {
        let help = parse(args).expect_err("help exits").to_string();
        assert!(help.trim_end().ends_with(AGENT_POINTER), "{help}");
        assert!(!help.contains("from: [x, y]"), "{help}");
    }
}
