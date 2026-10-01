use super::units::{Logical, Rect};

pub fn parse_geometry(text: &str) -> Result<Rect<Logical>, String> {
    let trimmed = text.trim();
    let (xy, wh) = if let Some(split) = trimmed.split_once(' ') {
        split
    } else {
        let mut parts = trimmed.rsplitn(2, ',');
        let wh = parts.next().ok_or_else(|| invalid(text))?;
        let xy = parts.next().ok_or_else(|| invalid(text))?;
        (xy, wh)
    };
    let (x, y) = xy.split_once(',').ok_or_else(|| invalid(text))?;
    let (w, h) = wh.split_once('x').ok_or_else(|| invalid(text))?;
    let x: f64 = x.trim().parse().map_err(|_| invalid(text))?;
    let y: f64 = y.trim().parse().map_err(|_| invalid(text))?;
    let w: f64 = w.trim().parse().map_err(|_| invalid(text))?;
    let h: f64 = h.trim().parse().map_err(|_| invalid(text))?;
    if w <= 0.0 || h <= 0.0 {
        return Err(format!("width and height must be positive: {text}"));
    }
    Ok(Rect {
        x: Logical(x),
        y: Logical(y),
        width: Logical(w),
        height: Logical(h),
    })
}

fn invalid(text: &str) -> String {
    format!("invalid geometry: {text}")
}

pub fn within_one_output(
    rect: Rect<Logical>,
    outputs: &[(String, Rect<Logical>)],
) -> Result<String, String> {
    if let Some(name) = outputs
        .iter()
        .find(|(_, output)| contains(*output, rect))
        .map(|(name, _)| name.clone())
    {
        return Ok(name);
    }

    let touching: Vec<&str> = outputs
        .iter()
        .filter(|(_, output)| intersects(*output, rect))
        .map(|(name, _)| name.as_str())
        .collect();
    if touching.is_empty() {
        Err("the region does not lie within any output".to_owned())
    } else {
        Err(format!(
            "region spans {}; one output per shot",
            touching.join(" and ")
        ))
    }
}

fn contains(output: Rect<Logical>, rect: Rect<Logical>) -> bool {
    rect.x.0 >= output.x.0
        && rect.y.0 >= output.y.0
        && rect.x.0 + rect.width.0 <= output.x.0 + output.width.0
        && rect.y.0 + rect.height.0 <= output.y.0 + output.height.0
}

fn intersects(output: Rect<Logical>, rect: Rect<Logical>) -> bool {
    rect.x.0 < output.x.0 + output.width.0
        && output.x.0 < rect.x.0 + rect.width.0
        && rect.y.0 < output.y.0 + output.height.0
        && output.y.0 < rect.y.0 + rect.height.0
}

#[cfg(test)]
mod tests;
