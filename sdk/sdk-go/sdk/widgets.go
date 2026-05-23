package sdk

import "encoding/json"

type Widget interface {
	isWidget()
	json.Marshaler
}

type InlineHandler func(CallbackEvent) error

type FontSize string
type FontWeight string
type TextColor string
type BadgeKind string
type StatusDotStatus string
type PagerAppearance string
type PopoverSize string

const (
	FontSizeXs   FontSize = "xs"
	FontSizeSm   FontSize = "sm"
	FontSizeBase FontSize = "base"
	FontSizeLg   FontSize = "lg"
	FontSizeXl   FontSize = "xl"

	FontWeightNormal   FontWeight = "normal"
	FontWeightMedium   FontWeight = "medium"
	FontWeightSemibold FontWeight = "semibold"
	FontWeightBold     FontWeight = "bold"

	TextColorNormal  TextColor = "normal"
	TextColorMuted   TextColor = "muted"
	TextColorAccent  TextColor = "accent"
	TextColorSuccess TextColor = "success"
	TextColorWarning TextColor = "warning"
	TextColorError   TextColor = "error"

	BadgeKindDefault BadgeKind = "default"
	BadgeKindSuccess BadgeKind = "success"
	BadgeKindWarning BadgeKind = "warning"
	BadgeKindError   BadgeKind = "error"
	BadgeKindAccent  BadgeKind = "accent"

	StatusDotNeutral StatusDotStatus = "neutral"
	StatusDotSuccess StatusDotStatus = "success"
	StatusDotWarning StatusDotStatus = "warning"
	StatusDotError   StatusDotStatus = "error"
	StatusDotAccent  StatusDotStatus = "accent"

	PagerAppearanceDots    PagerAppearance = "dots"
	PagerAppearanceNumbers PagerAppearance = "numbers"

	PopoverSizeSmall  PopoverSize = "small"
	PopoverSizeMedium PopoverSize = "medium"
	PopoverSizeLarge  PopoverSize = "large"
	PopoverSizeWide   PopoverSize = "wide"
)

type CommonProps struct {
	Visible    *bool             `json:"visible,omitempty"`
	Tooltip    string            `json:"tooltip,omitempty"`
	CssClasses []string          `json:"css_classes,omitempty"`
	Styles     map[string]string `json:"styles,omitempty"`
}

func envelope(kind string, data any) ([]byte, error) {
	return json.Marshal(struct {
		Type string `json:"type"`
		Data any    `json:"data"`
	}{kind, data})
}

type Text struct {
	CommonProps
	Text   string     `json:"text"`
	Size   FontSize   `json:"size,omitempty"`
	Weight FontWeight `json:"weight,omitempty"`
	Color  TextColor  `json:"color,omitempty"`
	XAlign *float64   `json:"xalign,omitempty"`
	Wrap   *bool      `json:"wrap,omitempty"`
}

func (Text) isWidget()                      {}
func (w Text) MarshalJSON() ([]byte, error) { type alias Text; return envelope("text", alias(w)) }

type Header struct {
	CommonProps
	Label string `json:"label"`
}

func (Header) isWidget()                      {}
func (w Header) MarshalJSON() ([]byte, error) { type alias Header; return envelope("header", alias(w)) }

type Hero struct {
	CommonProps
	ID              string        `json:"id,omitempty"`
	Title           string        `json:"title"`
	Subtitle        string        `json:"subtitle"`
	Icon            string        `json:"icon,omitempty"`
	IconSize        *int          `json:"icon_size,omitempty"`
	Toggle          *bool         `json:"toggle,omitempty"`
	ToggleSensitive *bool         `json:"toggle_sensitive,omitempty"`
	Separator       *bool         `json:"separator,omitempty"`
	Trailing        Widget        `json:"trailing,omitempty"`
	OnToggle        InlineHandler `json:"-"`
}

func (Hero) isWidget()                      {}
func (w Hero) MarshalJSON() ([]byte, error) { type alias Hero; return envelope("hero", alias(w)) }

type Badge struct {
	CommonProps
	Label string    `json:"label"`
	Kind  BadgeKind `json:"kind"`
}

func (Badge) isWidget() {}
func (w Badge) MarshalJSON() ([]byte, error) {
	if w.Kind == "" {
		w.Kind = BadgeKindDefault
	}
	type alias Badge
	return envelope("badge", alias(w))
}

type StatusDot struct {
	CommonProps
	Status StatusDotStatus `json:"status"`
}

func (StatusDot) isWidget() {}
func (w StatusDot) MarshalJSON() ([]byte, error) {
	if w.Status == "" {
		w.Status = StatusDotNeutral
	}
	type alias StatusDot
	return envelope("status_dot", alias(w))
}

type PanelIndicator struct {
	CommonProps
	ID             string        `json:"id,omitempty"`
	Icon           string        `json:"icon,omitempty"`
	Label          string        `json:"label,omitempty"`
	Active         bool          `json:"active"`
	Checked        bool          `json:"checked"`
	NeedsAttention bool          `json:"needs_attention"`
	Extra          Widget        `json:"extra,omitempty"`
	OnClick        InlineHandler `json:"-"`
}

func (PanelIndicator) isWidget() {}
func (w PanelIndicator) MarshalJSON() ([]byte, error) {
	type alias PanelIndicator
	return envelope("panel_indicator", alias(w))
}

type EmptyState struct {
	CommonProps
	Title    string `json:"title"`
	Subtitle string `json:"subtitle,omitempty"`
}

func (EmptyState) isWidget() {}
func (w EmptyState) MarshalJSON() ([]byte, error) {
	type alias EmptyState
	return envelope("empty_state", alias(w))
}

type Spinner struct {
	CommonProps
	Spinning *bool `json:"spinning,omitempty"`
}

type Meter struct {
	CommonProps
	ID          string        `json:"id,omitempty"`
	Icon        string        `json:"icon,omitempty"`
	Label       string        `json:"label"`
	Value       float64       `json:"value"`
	Min         float64       `json:"min"`
	Max         float64       `json:"max"`
	Step        float64       `json:"step"`
	Text        string        `json:"text,omitempty"`
	Interactive bool          `json:"interactive"`
	OnChange    InlineHandler `json:"-"`
}

type Separator struct {
	CommonProps
}

type Scroll struct {
	CommonProps
	Child Widget `json:"child"`
}

func (Spinner) isWidget()   {}
func (Meter) isWidget()     {}
func (Separator) isWidget() {}
func (Scroll) isWidget()    {}

func (w Spinner) MarshalJSON() ([]byte, error) {
	spinning := true
	if w.Spinning != nil {
		spinning = *w.Spinning
	}
	type data struct {
		CommonProps
		Spinning bool `json:"spinning"`
	}
	return envelope("spinner", data{CommonProps: w.CommonProps, Spinning: spinning})
}

func (w Meter) MarshalJSON() ([]byte, error) {
	if w.Max == 0 {
		w.Max = 1
	}
	if w.Step == 0 {
		w.Step = 0.01
	}
	if w.OnChange != nil {
		w.Interactive = true
	}
	type alias Meter
	return envelope("meter", alias(w))
}

func (w Separator) MarshalJSON() ([]byte, error) {
	type alias Separator
	return envelope("separator", alias(w))
}

func (w Scroll) MarshalJSON() ([]byte, error) {
	type alias Scroll
	return envelope("scroll", alias(w))
}

type Row struct {
	CommonProps
	Children []Widget `json:"children"`
}
type Column struct {
	CommonProps
	Children []Widget `json:"children"`
}
type BoxedList struct {
	CommonProps
	Children []Widget `json:"children"`
}
type ButtonRow struct {
	CommonProps
	Children []Widget `json:"children"`
}

func (Row) isWidget()       {}
func (Column) isWidget()    {}
func (BoxedList) isWidget() {}
func (ButtonRow) isWidget() {}
func (w Row) MarshalJSON() ([]byte, error) {
	type alias Row
	if w.Children == nil {
		w.Children = []Widget{}
	}
	return envelope("row", alias(w))
}
func (w Column) MarshalJSON() ([]byte, error) {
	type alias Column
	if w.Children == nil {
		w.Children = []Widget{}
	}
	return envelope("column", alias(w))
}
func (w BoxedList) MarshalJSON() ([]byte, error) {
	type alias BoxedList
	if w.Children == nil {
		w.Children = []Widget{}
	}
	return envelope("boxed_list", alias(w))
}
func (w ButtonRow) MarshalJSON() ([]byte, error) {
	type alias ButtonRow
	if w.Children == nil {
		w.Children = []Widget{}
	}
	return envelope("button_row", alias(w))
}

type Container struct {
	CommonProps
	Children []Widget `json:"children"`
}

func (Container) isWidget() {}
func (w Container) MarshalJSON() ([]byte, error) {
	if w.Children == nil {
		w.Children = []Widget{}
	}
	type alias Container
	return envelope("container", alias(w))
}

type CircleBox struct {
	CommonProps
	Color string `json:"color"`
}

func (CircleBox) isWidget() {}
func (w CircleBox) MarshalJSON() ([]byte, error) {
	type alias CircleBox
	return envelope("circle_box", alias(w))
}

type PopoverShell struct {
	CommonProps
	Size          PopoverSize `json:"size"`
	Children      []Widget    `json:"children"`
	Footer        []Widget    `json:"footer,omitempty"`
	FooterVisible bool        `json:"footer_visible,omitempty"`
}

func (PopoverShell) isWidget() {}
func (w PopoverShell) MarshalJSON() ([]byte, error) {
	if w.Size == "" {
		w.Size = PopoverSizeMedium
	}
	if w.Children == nil {
		w.Children = []Widget{}
	}
	type alias PopoverShell
	return envelope("popover_shell", alias(w))
}

type Tile struct {
	CommonProps
	ID          string        `json:"id,omitempty"`
	Primary     string        `json:"primary"`
	Secondary   string        `json:"secondary,omitempty"`
	LeftIcon    string        `json:"left_icon,omitempty"`
	Left        Widget        `json:"left,omitempty"`
	Right       Widget        `json:"right,omitempty"`
	Activatable bool          `json:"activatable"`
	OnClick     InlineHandler `json:"-"`
}

func (Tile) isWidget()                      {}
func (w Tile) MarshalJSON() ([]byte, error) { type alias Tile; return envelope("tile", alias(w)) }

type SegmentedTile struct {
	Tile
	Child    Widget        `json:"child,omitempty"`
	Expanded bool          `json:"expanded"`
	OnToggle InlineHandler `json:"-"`
}

func (SegmentedTile) isWidget() {}
func (w SegmentedTile) MarshalJSON() ([]byte, error) {
	type data struct {
		CommonProps
		ID          string `json:"id,omitempty"`
		Primary     string `json:"primary"`
		Secondary   string `json:"secondary,omitempty"`
		LeftIcon    string `json:"left_icon,omitempty"`
		Left        Widget `json:"left,omitempty"`
		Right       Widget `json:"right,omitempty"`
		Activatable bool   `json:"activatable"`
		Child       Widget `json:"child,omitempty"`
		Expanded    bool   `json:"expanded"`
	}
	return envelope("segmented_tile", data{w.CommonProps, w.ID, w.Primary, w.Secondary, w.LeftIcon, w.Left, w.Right, w.Activatable, w.Child, w.Expanded})
}

type SwitchTile struct {
	CommonProps
	ID        string        `json:"id"`
	Primary   string        `json:"primary"`
	Secondary string        `json:"secondary,omitempty"`
	LeftIcon  string        `json:"left_icon,omitempty"`
	Left      Widget        `json:"left,omitempty"`
	Active    bool          `json:"active"`
	OnToggle  InlineHandler `json:"-"`
}

func (SwitchTile) isWidget() {}
func (w SwitchTile) MarshalJSON() ([]byte, error) {
	type alias SwitchTile
	return envelope("switch_tile", alias(w))
}

type ExpanderTile struct {
	CommonProps
	ID        string        `json:"id,omitempty"`
	Primary   string        `json:"primary"`
	Secondary string        `json:"secondary,omitempty"`
	LeftIcon  string        `json:"left_icon,omitempty"`
	Left      Widget        `json:"left,omitempty"`
	Child     Widget        `json:"child,omitempty"`
	Expanded  bool          `json:"expanded"`
	OnToggle  InlineHandler `json:"-"`
}

func (ExpanderTile) isWidget() {}
func (w ExpanderTile) MarshalJSON() ([]byte, error) {
	type alias ExpanderTile
	return envelope("expander_tile", alias(w))
}

type SliderTile struct {
	CommonProps
	ID       string        `json:"id"`
	Label    string        `json:"label,omitempty"`
	LeftIcon string        `json:"left_icon,omitempty"`
	Left     Widget        `json:"left,omitempty"`
	Value    float64       `json:"value"`
	Min      float64       `json:"min"`
	Max      float64       `json:"max"`
	Step     float64       `json:"step"`
	Page     float64       `json:"page"`
	Digits   int           `json:"digits"`
	SnapStep *float64      `json:"snap_step,omitempty"`
	OnChange InlineHandler `json:"-"`
}

func (SliderTile) isWidget() {}
func (w SliderTile) MarshalJSON() ([]byte, error) {
	if w.Max == 0 {
		w.Max = 1
	}
	if w.Step == 0 {
		w.Step = 0.01
	}
	if w.Page == 0 {
		w.Page = 0.1
	}
	type alias SliderTile
	return envelope("slider_tile", alias(w))
}

type ChoiceTile struct {
	CommonProps
	ID        string        `json:"id,omitempty"`
	Primary   string        `json:"primary"`
	Secondary string        `json:"secondary,omitempty"`
	LeftIcon  string        `json:"left_icon,omitempty"`
	Left      Widget        `json:"left,omitempty"`
	Selected  bool          `json:"selected"`
	OnClick   InlineHandler `json:"-"`
}

func (ChoiceTile) isWidget() {}
func (w ChoiceTile) MarshalJSON() ([]byte, error) {
	type alias ChoiceTile
	return envelope("choice_tile", alias(w))
}

type Choice struct {
	ID        string `json:"id"`
	Primary   string `json:"primary"`
	Secondary string `json:"secondary,omitempty"`
	Icon      string `json:"icon,omitempty"`
}
type ChoiceList struct {
	CommonProps
	ID       string        `json:"id"`
	Active   string        `json:"active,omitempty"`
	Choices  []Choice      `json:"choices"`
	OnChange InlineHandler `json:"-"`
}

func (ChoiceList) isWidget() {}
func (w ChoiceList) MarshalJSON() ([]byte, error) {
	if w.Choices == nil {
		w.Choices = []Choice{}
	}
	type alias ChoiceList
	return envelope("choice_list", alias(w))
}

type KeyValueRow struct {
	Key   string `json:"key"`
	Value string `json:"value"`
}
type KeyValueGrid struct {
	CommonProps
	Rows []KeyValueRow `json:"rows"`
}

func (KeyValueGrid) isWidget() {}
func (w KeyValueGrid) MarshalJSON() ([]byte, error) {
	if w.Rows == nil {
		w.Rows = []KeyValueRow{}
	}
	type alias KeyValueGrid
	return envelope("key_value_grid", alias(w))
}

type PagerItem struct {
	CommonProps
	ID         uint64          `json:"id"`
	Label      string          `json:"label"`
	Appearance PagerAppearance `json:"appearance"`
	Active     bool            `json:"active"`
	Inactive   bool            `json:"inactive"`
	Occupied   bool            `json:"occupied"`
	Urgent     bool            `json:"urgent"`
	OnClick    InlineHandler   `json:"-"`
}

func (PagerItem) isWidget() {}

type pagerItemPayload PagerItem

func pagerItemData(w PagerItem) pagerItemPayload {
	if w.Appearance == "" {
		w.Appearance = PagerAppearanceDots
	}
	return pagerItemPayload(w)
}
func (w PagerItem) MarshalJSON() ([]byte, error) { return envelope("pager_item", pagerItemData(w)) }

type PagerStrip struct {
	CommonProps
	ID          string        `json:"id,omitempty"`
	Placeholder bool          `json:"placeholder"`
	Items       []PagerItem   `json:"items"`
	OnChange    InlineHandler `json:"-"`
}

func (PagerStrip) isWidget() {}
func (w PagerStrip) MarshalJSON() ([]byte, error) {
	items := make([]pagerItemPayload, 0, len(w.Items))
	for _, item := range w.Items {
		items = append(items, pagerItemData(item))
	}
	type data struct {
		CommonProps
		ID          string             `json:"id,omitempty"`
		Placeholder bool               `json:"placeholder"`
		Items       []pagerItemPayload `json:"items"`
	}
	return envelope("pager_strip", data{w.CommonProps, w.ID, w.Placeholder, items})
}

type ActiveIndicator struct {
	CommonProps
	Active bool `json:"active"`
}
type CameraIndicator struct{ ActiveIndicator }
type MicIndicator struct{ ActiveIndicator }
type MutedIndicator struct{ ActiveIndicator }
type LocationIndicator struct{ ActiveIndicator }

func (CameraIndicator) isWidget()   {}
func (MicIndicator) isWidget()      {}
func (MutedIndicator) isWidget()    {}
func (LocationIndicator) isWidget() {}
func (w CameraIndicator) MarshalJSON() ([]byte, error) {
	return envelope("camera_indicator", w.ActiveIndicator)
}
func (w MicIndicator) MarshalJSON() ([]byte, error) {
	return envelope("mic_indicator", w.ActiveIndicator)
}
func (w MutedIndicator) MarshalJSON() ([]byte, error) {
	return envelope("muted_indicator", w.ActiveIndicator)
}
func (w LocationIndicator) MarshalJSON() ([]byte, error) {
	return envelope("location_indicator", w.ActiveIndicator)
}

type ScreenCastIndicator struct {
	ActiveIndicator
	TimerText string `json:"timer_text,omitempty"`
}

func (ScreenCastIndicator) isWidget() {}
func (w ScreenCastIndicator) MarshalJSON() ([]byte, error) {
	type data struct {
		CommonProps
		Active    bool   `json:"active"`
		TimerText string `json:"timer_text,omitempty"`
	}
	return envelope("screencast_indicator", data{w.CommonProps, w.Active, w.TimerText})
}

type Calendar struct {
	CommonProps
	ID           string        `json:"id,omitempty"`
	SelectedDate string        `json:"selected_date"`
	EventDays    []string      `json:"event_days"`
	OnChange     InlineHandler `json:"-"`
}

func (Calendar) isWidget() {}
func (w Calendar) MarshalJSON() ([]byte, error) {
	if w.EventDays == nil {
		w.EventDays = []string{}
	}
	type alias Calendar
	return envelope("calendar", alias(w))
}

type BatteryHero struct {
	CommonProps
	Icon       string  `json:"icon"`
	Percentage string  `json:"percentage"`
	Fraction   float64 `json:"fraction"`
	State      string  `json:"state"`
}

func (BatteryHero) isWidget() {}
func (w BatteryHero) MarshalJSON() ([]byte, error) {
	type alias BatteryHero
	return envelope("battery_hero", alias(w))
}

type DateHero struct {
	CommonProps
	Weekday string `json:"weekday"`
	Date    string `json:"date"`
}

func (DateHero) isWidget() {}
func (w DateHero) MarshalJSON() ([]byte, error) {
	type alias DateHero
	return envelope("date_hero", alias(w))
}

type EventItem struct {
	ID       string `json:"id"`
	Title    string `json:"title"`
	Start    string `json:"start"`
	End      string `json:"end"`
	Location string `json:"location,omitempty"`
	AllDay   bool   `json:"all_day"`
}
type Events struct {
	CommonProps
	Date    string      `json:"date"`
	Events  []EventItem `json:"events"`
	Loading bool        `json:"loading"`
}

func (Events) isWidget() {}
func (w Events) MarshalJSON() ([]byte, error) {
	if w.Events == nil {
		w.Events = []EventItem{}
	}
	type alias Events
	return envelope("events", alias(w))
}

type WeatherForecastItem struct {
	DayName      string `json:"day_name"`
	Icon         string `json:"icon"`
	Condition    string `json:"condition"`
	Temperatures string `json:"temperatures"`
	IsToday      bool   `json:"is_today"`
}
type WeatherForecastList struct {
	CommonProps
	Items []WeatherForecastItem `json:"items"`
}

func (WeatherForecastList) isWidget() {}
func (w WeatherForecastList) MarshalJSON() ([]byte, error) {
	if w.Items == nil {
		w.Items = []WeatherForecastItem{}
	}
	type alias WeatherForecastList
	return envelope("weather_forecast_list", alias(w))
}

type WeatherHourlyItem struct {
	Time        string `json:"time"`
	Icon        string `json:"icon"`
	Temperature string `json:"temperature"`
}
type WeatherHourlyStrip struct {
	CommonProps
	Items []WeatherHourlyItem `json:"items"`
}

func (WeatherHourlyStrip) isWidget() {}
func (w WeatherHourlyStrip) MarshalJSON() ([]byte, error) {
	if w.Items == nil {
		w.Items = []WeatherHourlyItem{}
	}
	type alias WeatherHourlyStrip
	return envelope("weather_hourly_strip", alias(w))
}

type WorldClockRow struct {
	Name     string `json:"name"`
	Timezone string `json:"timezone"`
	Time     string `json:"time"`
	Offset   string `json:"offset"`
	DayLabel string `json:"day_label,omitempty"`
}
type WorldClock struct {
	CommonProps
	Rows []WorldClockRow `json:"rows"`
}

func (WorldClock) isWidget() {}
func (w WorldClock) MarshalJSON() ([]byte, error) {
	if w.Rows == nil {
		w.Rows = []WorldClockRow{}
	}
	type alias WorldClock
	return envelope("world_clock", alias(w))
}
