use crate::widget::Widget;

macro_rules! kinds {
    ($($name:ident => $kind:literal),* $(,)?) => {
        $(
            pub fn $name<M>() -> Widget<M> {
                Widget::new($kind)
            }
        )*
    };
}

kinds! {
    indicator => "indicator", popover => "popover", addon => "addon",
    container => "container", section => "section", row => "row",
    action_row => "actionrow", toggle_row => "togglerow", meter_row => "meterrow",
    expander_row => "expanderrow", slider_row => "sliderrow", start => "start",
    end => "end", entry => "entry", placeholder => "placeholder", bx => "box",
    label => "label", image => "image", button => "button", switch => "switch",
    scale => "scale", spinner => "spinner", separator => "separator",
    fact_list => "factlist", choice_list => "choicelist", readout => "readout",
    notice => "notice", calendar => "calendar", dots => "dots",
    drop_down => "dropdown", toggle_button => "togglebutton",
    check_button => "checkbutton", revealer => "revealer", stack => "stack",
    range_bar => "rangebar", bars => "bars", sparkline => "sparkline",
    swatch => "swatch",
}
