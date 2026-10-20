use anyhow::Result;

use crate::session::{Outcome, Segment, Session, Settings};

pub fn measure(settings: Settings) -> Result<Vec<Segment>> {
    let outcome = crate::overlay::run(
        |frames, done| Session::open(frames, settings, done),
        Session::close,
    )?;
    Ok(confirmed(outcome))
}

fn confirmed(outcome: Option<Outcome>) -> Vec<Segment> {
    match outcome {
        Some(Outcome::Ended(segments)) => segments,
        Some(Outcome::Invalidated) => {
            tracing::info!("an output went away, so the session is discarded");
            Vec::new()
        }
        None => Vec::new(),
    }
}

pub fn last_distance(segments: &[Segment]) -> Option<String> {
    segments
        .last()
        .map(|segment| format!("{:.0}", segment.distance()))
}

pub fn ndjson(segments: &[Segment]) -> Option<String> {
    if segments.is_empty() {
        return None;
    }
    Some(
        segments
            .iter()
            .map(|segment| json(*segment) + "\n")
            .collect(),
    )
}

fn json(segment: Segment) -> String {
    let (dx, dy) = segment.delta();
    serde_json::json!({
        "from_x": segment.from.0,
        "from_y": segment.from.1,
        "to_x": segment.to.0,
        "to_y": segment.to.1,
        "dx": dx,
        "dy": dy,
        "distance": segment.distance(),
        "angle": segment.angle(),
    })
    .to_string()
}

#[cfg(test)]
mod tests;
