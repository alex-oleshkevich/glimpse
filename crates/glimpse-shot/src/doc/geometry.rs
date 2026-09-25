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
mod tests {
    use super::*;

    #[test]
    fn parse_geometry_accepts_the_slurp_form_and_the_comma_form() {
        let a = parse_geometry("100,200 300x400").expect("slurp form");
        assert_eq!(
            a,
            Rect {
                x: Logical(100.0),
                y: Logical(200.0),
                width: Logical(300.0),
                height: Logical(400.0),
            }
        );

        let b = parse_geometry("100,200,300x400").expect("comma form");
        assert_eq!(b, a);
    }

    #[test]
    fn parse_geometry_accepts_negative_x_and_y() {
        let rect = parse_geometry("-100,-50 300x400").expect("negative origin");
        assert_eq!(rect.x, Logical(-100.0));
        assert_eq!(rect.y, Logical(-50.0));
    }

    #[test]
    fn parse_geometry_rejects_non_positive_width_or_height() {
        assert!(parse_geometry("0,0 0x400").is_err());
        assert!(parse_geometry("0,0 300x0").is_err());
        assert!(parse_geometry("0,0 -10x400").is_err());
    }

    #[test]
    fn parse_geometry_rejects_garbage() {
        assert!(parse_geometry("nonsense").is_err());
        assert!(parse_geometry("100,200").is_err());
    }

    #[test]
    fn within_one_output_refuses_spanning_and_touching_none_and_accepts_a_negative_x_output() {
        let outputs = vec![
            (
                "DP-2".to_owned(),
                Rect {
                    x: Logical(-3072.0),
                    y: Logical(0.0),
                    width: Logical(3072.0),
                    height: Logical(1728.0),
                },
            ),
            (
                "eDP-1".to_owned(),
                Rect {
                    x: Logical(0.0),
                    y: Logical(0.0),
                    width: Logical(2304.0),
                    height: Logical(1440.0),
                },
            ),
        ];

        let inside = Rect {
            x: Logical(-3000.0),
            y: Logical(10.0),
            width: Logical(100.0),
            height: Logical(100.0),
        };
        assert_eq!(within_one_output(inside, &outputs), Ok("DP-2".to_owned()));

        let spanning = Rect {
            x: Logical(-10.0),
            y: Logical(10.0),
            width: Logical(100.0),
            height: Logical(100.0),
        };
        assert!(within_one_output(spanning, &outputs).is_err());

        let outside = Rect {
            x: Logical(5000.0),
            y: Logical(5000.0),
            width: Logical(10.0),
            height: Logical(10.0),
        };
        assert!(within_one_output(outside, &outputs).is_err());
    }

    #[test]
    fn a_rect_crossing_the_boundary_between_two_tiled_outputs_names_both() {
        let outputs = vec![
            (
                "DP-2".to_owned(),
                Rect {
                    x: Logical(-3072.0),
                    y: Logical(0.0),
                    width: Logical(3072.0),
                    height: Logical(1728.0),
                },
            ),
            (
                "eDP-1".to_owned(),
                Rect {
                    x: Logical(0.0),
                    y: Logical(0.0),
                    width: Logical(2304.0),
                    height: Logical(1440.0),
                },
            ),
        ];

        let crossing_zero = Rect {
            x: Logical(-10.0),
            y: Logical(10.0),
            width: Logical(20.0),
            height: Logical(20.0),
        };

        assert_eq!(
            within_one_output(crossing_zero, &outputs),
            Err("region spans DP-2 and eDP-1; one output per shot".to_owned())
        );
    }
}
