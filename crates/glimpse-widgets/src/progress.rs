use gtk4::prelude::*;

pub(crate) fn apply_bar(cell: &gtk4::Box, fraction: Option<f64>, class: &str) {
    let fraction = fraction.filter(|fraction| fraction.is_finite());
    match fraction {
        Some(fraction) => {
            let bar = match cell.last_child().and_downcast::<gtk4::ProgressBar>() {
                Some(bar) => bar,
                None => {
                    let bar = gtk4::ProgressBar::new();
                    bar.add_css_class(class);
                    cell.append(&bar);
                    bar
                }
            };
            if bar.fraction() != fraction {
                bar.set_fraction(fraction);
            }
        }
        None => {
            if let Some(bar) = cell.last_child().and_downcast::<gtk4::ProgressBar>() {
                bar.unparent();
            }
        }
    }
}
