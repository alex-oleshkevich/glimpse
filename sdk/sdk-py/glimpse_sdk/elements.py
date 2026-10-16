import collections.abc
import typing

from glimpse_sdk.widget import Child, Key, W

ClassName = typing.Literal[
    "dim-label",
    "caption",
    "heading",
    "title-1",
    "title-2",
    "title-3",
    "title-4",
    "numeric",
    "accent",
    "success",
    "warning",
    "error",
    "flat",
    "pill",
    "circular",
    "column",
    "network-popover__rule",
    "bluetooth-popover",
    "network-popover",
]
Tone = typing.Literal["accent", "dim", "success", "warning", "error"]
RowTone = typing.Literal["warning", "error", "destructive"]
RowTypography = typing.Literal["numeric", "monospace"]
Severity = typing.Literal["info", "warning", "error"]
Paint = typing.Literal["red", "orange", "yellow", "green", "blue", "purple", "light", "dark"]
Align = typing.Literal["fill", "start", "end", "center", "baseline"]
PopoverSize = typing.Literal["regular", "wide"]
ToggleControl = typing.Literal["check", "switch"]
Orientation = typing.Literal["horizontal", "vertical"]
Ellipsize = typing.Literal["none", "start", "middle", "end"]
SwatchSize = typing.Literal["regular", "large"]
Transition = typing.Literal["slide-down", "crossfade", "none"]
Urgency = typing.Literal["low", "normal", "critical"]

__all__ = [
    "ActionRow",
    "Addon",
    "Align",
    "Bars",
    "Box",
    "Button",
    "Calendar",
    "CalendarEvent",
    "CheckButton",
    "Choice",
    "ChoiceList",
    "ClassName",
    "Container",
    "Dots",
    "DropDown",
    "Ellipsize",
    "End",
    "Entry",
    "ExpanderRow",
    "Fact",
    "FactList",
    "Image",
    "Indicator",
    "Label",
    "MeterRow",
    "Notice",
    "Orientation",
    "Paint",
    "Placeholder",
    "Popover",
    "PopoverSize",
    "RangeBar",
    "Readout",
    "Revealer",
    "Row",
    "RowTone",
    "RowTypography",
    "Scale",
    "Section",
    "Separator",
    "Severity",
    "SliderRow",
    "Sparkline",
    "Spinner",
    "Stack",
    "Start",
    "Swatch",
    "SwatchSize",
    "Switch",
    "ToggleButton",
    "ToggleControl",
    "ToggleRow",
    "Tone",
    "Transition",
    "Urgency",
]


class Fact(typing.TypedDict, total=False):
    label: str
    value: str
    warning: bool


class Choice(typing.TypedDict, total=False):
    label: str
    detail: str
    icon: str
    warning: bool


class CalendarEvent(typing.TypedDict, total=False):
    date: str
    colors: collections.abc.Sequence[Tone]


def Indicator(
    *body: Child,
    key: Key = None,
    icon: str | None = None,
    text: str | None = None,
    tooltip: str | None = None,
    badge: str | None = None,
    overlay: str | None = None,
    dot: str | None = None,
    severity: Severity | None = None,
    attention: bool | None = None,
    notice: bool | None = None,
    class_name: ClassName | None = None,
    on_press: collections.abc.Callable[[int], object] | None = None,
    on_scroll: collections.abc.Callable[[float, float], object] | None = None,
    children: Child = (),
) -> W:
    return W(
        "indicator",
        (*body, children),
        key,
        icon=icon,
        text=text,
        tooltip=tooltip,
        badge=badge,
        overlay=overlay,
        dot=dot,
        severity=severity,
        attention=attention,
        notice=notice,
        class_name=class_name,
        on_press=on_press,
        on_scroll=on_scroll,
    )


def Popover(
    *body: Child,
    key: Key = None,
    class_name: ClassName | None = None,
    icon: str | None = None,
    title: str | None = None,
    subtitle: str | None = None,
    size: PopoverSize | None = None,
    children: Child = (),
) -> W:
    return W(
        "popover",
        (*body, children),
        key,
        class_name=class_name,
        icon=icon,
        title=title,
        subtitle=subtitle,
        size=size,
    )


def Addon(
    *body: Child,
    key: Key = None,
    children: Child = (),
) -> W:
    return W(
        "addon",
        (*body, children),
        key,
    )


def Container(
    *body: Child,
    key: Key = None,
    width: int | None | None = None,
    height: int | None | None = None,
    background: Paint | None = None,
    color: Paint | None = None,
    align: Align | None = None,
    children: Child = (),
) -> W:
    return W(
        "container",
        (*body, children),
        key,
        width=width,
        height=height,
        background=background,
        color=color,
        align=align,
    )


def Section(
    *body: Child,
    key: Key = None,
    title: str | None = None,
    count: str | None = None,
    class_name: ClassName | None = None,
    children: Child = (),
) -> W:
    return W(
        "section",
        (*body, children),
        key,
        title=title,
        count=count,
        class_name=class_name,
    )


def Row(
    *body: Child,
    key: Key = None,
    icon: str | None = None,
    title: str | None = None,
    subtitle: str | None = None,
    value: str | None = None,
    tone: RowTone | None = None,
    inactive: bool | None = None,
    tooltip: str | None = None,
    typography: RowTypography | None = None,
    children: Child = (),
) -> W:
    return W(
        "row",
        (*body, children),
        key,
        icon=icon,
        title=title,
        subtitle=subtitle,
        value=value,
        tone=tone,
        inactive=inactive,
        tooltip=tooltip,
        typography=typography,
    )


def ActionRow(
    *body: Child,
    key: Key = None,
    icon: str | None = None,
    title: str | None = None,
    subtitle: str | None = None,
    value: str | None = None,
    tone: RowTone | None = None,
    inactive: bool | None = None,
    tooltip: str | None = None,
    typography: RowTypography | None = None,
    busy: bool | None = None,
    on_activate: collections.abc.Callable[[], object] | None = None,
    children: Child = (),
) -> W:
    return W(
        "actionrow",
        (*body, children),
        key,
        icon=icon,
        title=title,
        subtitle=subtitle,
        value=value,
        tone=tone,
        inactive=inactive,
        tooltip=tooltip,
        typography=typography,
        busy=busy,
        on_activate=on_activate,
    )


def ToggleRow(
    *body: Child,
    key: Key = None,
    icon: str | None = None,
    title: str | None = None,
    subtitle: str | None = None,
    value: str | None = None,
    tone: RowTone | None = None,
    inactive: bool | None = None,
    tooltip: str | None = None,
    typography: RowTypography | None = None,
    busy: bool | None = None,
    control: ToggleControl | None = None,
    active: bool | None = None,
    on_toggle: collections.abc.Callable[[bool], object] | None = None,
    children: Child = (),
) -> W:
    return W(
        "togglerow",
        (*body, children),
        key,
        icon=icon,
        title=title,
        subtitle=subtitle,
        value=value,
        tone=tone,
        inactive=inactive,
        tooltip=tooltip,
        typography=typography,
        busy=busy,
        control=control,
        active=active,
        on_toggle=on_toggle,
    )


def MeterRow(
    *body: Child,
    key: Key = None,
    icon: str | None = None,
    title: str | None = None,
    subtitle: str | None = None,
    value: str | None = None,
    tone: RowTone | None = None,
    inactive: bool | None = None,
    tooltip: str | None = None,
    typography: RowTypography | None = None,
    busy: bool | None = None,
    level: float | None | None = None,
    children: Child = (),
) -> W:
    return W(
        "meterrow",
        (*body, children),
        key,
        icon=icon,
        title=title,
        subtitle=subtitle,
        value=value,
        tone=tone,
        inactive=inactive,
        tooltip=tooltip,
        typography=typography,
        busy=busy,
        level=level,
    )


def ExpanderRow(
    *body: Child,
    key: Key = None,
    icon: str | None = None,
    title: str | None = None,
    subtitle: str | None = None,
    value: str | None = None,
    tone: RowTone | None = None,
    inactive: bool | None = None,
    tooltip: str | None = None,
    typography: RowTypography | None = None,
    busy: bool | None = None,
    open: bool | None = None,
    tint: bool | None = None,
    title_lines: int | None = None,
    on_toggle: collections.abc.Callable[[bool], object] | None = None,
    children: Child = (),
) -> W:
    return W(
        "expanderrow",
        (*body, children),
        key,
        icon=icon,
        title=title,
        subtitle=subtitle,
        value=value,
        tone=tone,
        inactive=inactive,
        tooltip=tooltip,
        typography=typography,
        busy=busy,
        open=open,
        tint=tint,
        title_lines=title_lines,
        on_toggle=on_toggle,
    )


def SliderRow(
    *body: Child,
    key: Key = None,
    title: str | None = None,
    subtitle: str | None = None,
    value: float | None = None,
    min: float | None = None,
    max: float | None = None,
    step: float | None = None,
    tone: RowTone | None = None,
    inactive: bool | None = None,
    tooltip: str | None = None,
    on_move: collections.abc.Callable[[float], object] | None = None,
    on_change: collections.abc.Callable[[float], object] | None = None,
    children: Child = (),
) -> W:
    return W(
        "sliderrow",
        (*body, children),
        key,
        title=title,
        subtitle=subtitle,
        value=value,
        min=min,
        max=max,
        step=step,
        tone=tone,
        inactive=inactive,
        tooltip=tooltip,
        on_move=on_move,
        on_change=on_change,
    )


def Start(
    *body: Child,
    key: Key = None,
    children: Child = (),
) -> W:
    return W(
        "start",
        (*body, children),
        key,
    )


def End(
    *body: Child,
    key: Key = None,
    children: Child = (),
) -> W:
    return W(
        "end",
        (*body, children),
        key,
    )


def Entry(
    *body: Child,
    key: Key = None,
    placeholder: str | None = None,
    value: str | None = None,
    class_name: ClassName | None = None,
    on_change: collections.abc.Callable[[str], object] | None = None,
    on_submit: collections.abc.Callable[[str], object] | None = None,
    children: Child = (),
) -> W:
    return W(
        "entry",
        (*body, children),
        key,
        placeholder=placeholder,
        value=value,
        class_name=class_name,
        on_change=on_change,
        on_submit=on_submit,
    )


def Placeholder(
    *body: Child,
    key: Key = None,
    icon: str | None = None,
    title: str | None = None,
    description: str | None = None,
    class_name: ClassName | None = None,
    children: Child = (),
) -> W:
    return W(
        "placeholder",
        (*body, children),
        key,
        icon=icon,
        title=title,
        description=description,
        class_name=class_name,
    )


def Box(
    *body: Child,
    key: Key = None,
    orientation: Orientation | None = None,
    spacing: int | None = None,
    homogeneous: bool | None = None,
    halign: Align | None = None,
    valign: Align | None = None,
    hexpand: bool | None = None,
    vexpand: bool | None = None,
    class_name: ClassName | None = None,
    children: Child = (),
) -> W:
    return W(
        "box",
        (*body, children),
        key,
        orientation=orientation,
        spacing=spacing,
        homogeneous=homogeneous,
        halign=halign,
        valign=valign,
        hexpand=hexpand,
        vexpand=vexpand,
        class_name=class_name,
    )


def Label(
    *body: Child,
    key: Key = None,
    text: str | None = None,
    wrap: bool | None = None,
    xalign: float | None | None = None,
    ellipsize: Ellipsize | None = None,
    lines: int | None = None,
    class_name: ClassName | None = None,
    children: Child = (),
) -> W:
    return W(
        "label",
        (*body, children),
        key,
        text=text,
        wrap=wrap,
        xalign=xalign,
        ellipsize=ellipsize,
        lines=lines,
        class_name=class_name,
    )


def Image(
    *body: Child,
    key: Key = None,
    icon: str | None = None,
    tooltip: str | None = None,
    pixel_size: int | None | None = None,
    class_name: ClassName | None = None,
    children: Child = (),
) -> W:
    return W(
        "image",
        (*body, children),
        key,
        icon=icon,
        tooltip=tooltip,
        pixel_size=pixel_size,
        class_name=class_name,
    )


def Button(
    *body: Child,
    key: Key = None,
    text: str | None = None,
    icon: str | None = None,
    tooltip: str | None = None,
    sensitive: bool | None | None = None,
    class_name: ClassName | None = None,
    on_click: collections.abc.Callable[[], object] | None = None,
    children: Child = (),
) -> W:
    return W(
        "button",
        (*body, children),
        key,
        text=text,
        icon=icon,
        tooltip=tooltip,
        sensitive=sensitive,
        class_name=class_name,
        on_click=on_click,
    )


def Switch(
    *body: Child,
    key: Key = None,
    active: bool | None = None,
    sensitive: bool | None | None = None,
    class_name: ClassName | None = None,
    on_toggle: collections.abc.Callable[[bool], object] | None = None,
    children: Child = (),
) -> W:
    return W(
        "switch",
        (*body, children),
        key,
        active=active,
        sensitive=sensitive,
        class_name=class_name,
        on_toggle=on_toggle,
    )


def Scale(
    *body: Child,
    key: Key = None,
    value: float | None = None,
    min: float | None = None,
    max: float | None = None,
    step: float | None = None,
    sensitive: bool | None | None = None,
    class_name: ClassName | None = None,
    on_change: collections.abc.Callable[[float], object] | None = None,
    children: Child = (),
) -> W:
    return W(
        "scale",
        (*body, children),
        key,
        value=value,
        min=min,
        max=max,
        step=step,
        sensitive=sensitive,
        class_name=class_name,
        on_change=on_change,
    )


def Spinner(
    *body: Child,
    key: Key = None,
    class_name: ClassName | None = None,
    children: Child = (),
) -> W:
    return W(
        "spinner",
        (*body, children),
        key,
        class_name=class_name,
    )


def Separator(
    *body: Child,
    key: Key = None,
    orientation: Orientation | None = None,
    class_name: ClassName | None = None,
    children: Child = (),
) -> W:
    return W(
        "separator",
        (*body, children),
        key,
        orientation=orientation,
        class_name=class_name,
    )


def FactList(
    *body: Child,
    key: Key = None,
    facts: collections.abc.Sequence[Fact] | None = None,
    children: Child = (),
) -> W:
    return W(
        "factlist",
        (*body, children),
        key,
        facts=facts,
    )


def ChoiceList(
    *body: Child,
    key: Key = None,
    choices: collections.abc.Sequence[Choice] | None = None,
    selected: int | None | None = None,
    class_name: ClassName | None = None,
    on_select: collections.abc.Callable[[int], object] | None = None,
    children: Child = (),
) -> W:
    return W(
        "choicelist",
        (*body, children),
        key,
        choices=choices,
        selected=selected,
        class_name=class_name,
        on_select=on_select,
    )


def Readout(
    *body: Child,
    key: Key = None,
    value: str | None = None,
    unit: str | None = None,
    class_name: ClassName | None = None,
    children: Child = (),
) -> W:
    return W(
        "readout",
        (*body, children),
        key,
        value=value,
        unit=unit,
        class_name=class_name,
    )


def Notice(
    *body: Child,
    key: Key = None,
    icon: str | None = None,
    title: str | None = None,
    subtitle: str | None = None,
    severity: Severity | None = None,
    class_name: ClassName | None = None,
    on_activate: collections.abc.Callable[[], object] | None = None,
    children: Child = (),
) -> W:
    return W(
        "notice",
        (*body, children),
        key,
        icon=icon,
        title=title,
        subtitle=subtitle,
        severity=severity,
        class_name=class_name,
        on_activate=on_activate,
    )


def Calendar(
    *body: Child,
    key: Key = None,
    year: int | None = None,
    month: int | None = None,
    selected: str | None = None,
    today: str | None = None,
    events: collections.abc.Sequence[CalendarEvent] | None = None,
    class_name: ClassName | None = None,
    on_month: collections.abc.Callable[[int, int], object] | None = None,
    on_select: collections.abc.Callable[[str], object] | None = None,
    children: Child = (),
) -> W:
    return W(
        "calendar",
        (*body, children),
        key,
        year=year,
        month=month,
        selected=selected,
        today=today,
        events=events,
        class_name=class_name,
        on_month=on_month,
        on_select=on_select,
    )


def Dots(
    *body: Child,
    key: Key = None,
    colors: collections.abc.Sequence[Tone] | None = None,
    uniform: bool | None = None,
    size: int | None = None,
    max: int | None = None,
    class_name: ClassName | None = None,
    children: Child = (),
) -> W:
    return W(
        "dots",
        (*body, children),
        key,
        colors=colors,
        uniform=uniform,
        size=size,
        max=max,
        class_name=class_name,
    )


def DropDown(
    *body: Child,
    key: Key = None,
    options: collections.abc.Sequence[str] | None = None,
    selected: int | None | None = None,
    class_name: ClassName | None = None,
    on_select: collections.abc.Callable[[int], object] | None = None,
    children: Child = (),
) -> W:
    return W(
        "dropdown",
        (*body, children),
        key,
        options=options,
        selected=selected,
        class_name=class_name,
        on_select=on_select,
    )


def ToggleButton(
    *body: Child,
    key: Key = None,
    label: str | None = None,
    icon: str | None = None,
    active: bool | None = None,
    class_name: ClassName | None = None,
    on_toggle: collections.abc.Callable[[bool], object] | None = None,
    children: Child = (),
) -> W:
    return W(
        "togglebutton",
        (*body, children),
        key,
        label=label,
        icon=icon,
        active=active,
        class_name=class_name,
        on_toggle=on_toggle,
    )


def CheckButton(
    *body: Child,
    key: Key = None,
    label: str | None = None,
    active: bool | None = None,
    class_name: ClassName | None = None,
    on_toggle: collections.abc.Callable[[bool], object] | None = None,
    children: Child = (),
) -> W:
    return W(
        "checkbutton",
        (*body, children),
        key,
        label=label,
        active=active,
        class_name=class_name,
        on_toggle=on_toggle,
    )


def Revealer(
    *body: Child,
    key: Key = None,
    revealed: bool | None = None,
    transition: Transition | None = None,
    class_name: ClassName | None = None,
    children: Child = (),
) -> W:
    return W(
        "revealer",
        (*body, children),
        key,
        revealed=revealed,
        transition=transition,
        class_name=class_name,
    )


def Stack(
    *body: Child,
    key: Key = None,
    visible: int | None = None,
    class_name: ClassName | None = None,
    children: Child = (),
) -> W:
    return W(
        "stack",
        (*body, children),
        key,
        visible=visible,
        class_name=class_name,
    )


def RangeBar(
    *body: Child,
    key: Key = None,
    min: float | None = None,
    max: float | None = None,
    low: float | None = None,
    high: float | None = None,
    now: float | None | None = None,
    class_name: ClassName | None = None,
    children: Child = (),
) -> W:
    return W(
        "rangebar",
        (*body, children),
        key,
        min=min,
        max=max,
        low=low,
        high=high,
        now=now,
        class_name=class_name,
    )


def Bars(
    *body: Child,
    key: Key = None,
    values: collections.abc.Sequence[float | None] | None = None,
    maximum: float | None | None = None,
    highlight_last: bool | None | None = None,
    label: str | None = None,
    tooltips: collections.abc.Sequence[str | None] | None = None,
    class_name: ClassName | None = None,
    children: Child = (),
) -> W:
    return W(
        "bars",
        (*body, children),
        key,
        values=values,
        maximum=maximum,
        highlight_last=highlight_last,
        label=label,
        tooltips=tooltips,
        class_name=class_name,
    )


def Sparkline(
    *body: Child,
    key: Key = None,
    values: collections.abc.Sequence[float | None] | None = None,
    fill: bool | None = None,
    show_last: bool | None = None,
    label: str | None = None,
    class_name: ClassName | None = None,
    children: Child = (),
) -> W:
    return W(
        "sparkline",
        (*body, children),
        key,
        values=values,
        fill=fill,
        show_last=show_last,
        label=label,
        class_name=class_name,
    )


def Swatch(
    *body: Child,
    key: Key = None,
    color: str | None = None,
    tooltip: str | None = None,
    size: SwatchSize | None = None,
    children: Child = (),
) -> W:
    return W(
        "swatch",
        (*body, children),
        key,
        color=color,
        tooltip=tooltip,
        size=size,
    )
