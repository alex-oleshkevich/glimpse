package sdk

func Ptr[T any](v T) *T { return &v }

type ClassName string
type Tone string
type RowTone string
type RowTypography string
type Severity string
type Paint string
type Align string
type PopoverSize string
type ToggleControl string
type Orientation string
type Ellipsize string
type SwatchSize string
type Transition string
type Urgency string

type Notification struct {
	Summary string
	Body    string
	Icon    string
	Urgency Urgency
}

type IndicatorProps struct {
	Key       any
	Icon      string
	Text      string
	Tooltip   string
	Badge     string
	Overlay   string
	Dot       string
	Severity  Severity
	Attention bool
	Notice    bool
	ClassName ClassName
	OnPress   func(button int)
	OnScroll  func(dx, dy float64)
}

type PopoverProps struct {
	Key       any
	ClassName ClassName
	Icon      string
	Title     string
	Subtitle  string
	Size      PopoverSize
}

type AddonProps struct {
	Key any
}

type ContainerProps struct {
	Key        any
	Width      *int
	Height     *int
	Background Paint
	Color      Paint
	Align      Align
}

type SectionProps struct {
	Key       any
	Title     string
	Count     string
	ClassName ClassName
}

type RowProps struct {
	Key        any
	Icon       string
	Title      string
	Subtitle   string
	Value      string
	Tone       RowTone
	Inactive   bool
	Tooltip    string
	Typography RowTypography
}

type ActionRowProps struct {
	Key        any
	Icon       string
	Title      string
	Subtitle   string
	Value      string
	Tone       RowTone
	Inactive   bool
	Tooltip    string
	Typography RowTypography
	Busy       bool
	OnActivate func()
}

type ToggleRowProps struct {
	Key        any
	Icon       string
	Title      string
	Subtitle   string
	Value      string
	Tone       RowTone
	Inactive   bool
	Tooltip    string
	Typography RowTypography
	Busy       bool
	Control    ToggleControl
	Active     bool
	OnToggle   func(active bool)
}

type MeterRowProps struct {
	Key        any
	Icon       string
	Title      string
	Subtitle   string
	Value      string
	Tone       RowTone
	Inactive   bool
	Tooltip    string
	Typography RowTypography
	Busy       bool
	Level      *float64
}

type ExpanderRowProps struct {
	Key        any
	Icon       string
	Title      string
	Subtitle   string
	Value      string
	Tone       RowTone
	Inactive   bool
	Tooltip    string
	Typography RowTypography
	Busy       bool
	Open       bool
	Tint       bool
	TitleLines int
	OnToggle   func(open bool)
}

type StartProps struct {
	Key any
}

type EndProps struct {
	Key any
}

type SliderRowProps struct {
	Key      any
	Title    string
	Subtitle string
	Value    float64 `glimpse:"value,always"`
	Min      float64
	Max      float64
	Step     float64
	Tone     RowTone
	Inactive bool
	Tooltip  string
	OnMove   func(value float64)
	OnChange func(value float64)
}

type EntryProps struct {
	Key         any
	Placeholder string
	Value       string
	ClassName   ClassName
	OnChange    func(value string)
	OnSubmit    func(value string)
}

type EntryRowProps struct {
	Key         any
	Title       string
	Placeholder string
	Value       string
	Tone        RowTone
	Inactive    bool
	Tooltip     string
	OnChange    func(value string)
	OnSubmit    func(value string)
}

type PlaceholderProps struct {
	Key         any
	Icon        string
	Title       string
	Description string
	ClassName   ClassName
}

type BoxProps struct {
	Key         any
	Orientation Orientation
	Spacing     int
	Homogeneous bool
	Halign      Align
	Valign      Align
	Hexpand     bool
	Vexpand     bool
	ClassName   ClassName
}

type LabelProps struct {
	Key       any
	Text      string
	Wrap      bool
	Xalign    *float64
	Ellipsize Ellipsize
	Lines     int
	ClassName ClassName
}

type ImageProps struct {
	Key       any
	Icon      string
	Tooltip   string
	PixelSize *int
	ClassName ClassName
}

type SwatchProps struct {
	Key     any
	Color   string
	Tooltip string
	Size    SwatchSize
}

type ButtonProps struct {
	Key       any
	Text      string
	Icon      string
	Tooltip   string
	Sensitive *bool
	ClassName ClassName
	OnClick   func()
}

type SwitchProps struct {
	Key       any
	Active    bool
	Sensitive *bool
	ClassName ClassName
	OnToggle  func(active bool)
}

type ScaleProps struct {
	Key       any
	Value     float64 `glimpse:"value,always"`
	Min       float64
	Max       float64
	Step      float64
	Sensitive *bool
	ClassName ClassName
	OnChange  func(value float64)
}

type SpinnerProps struct {
	Key       any
	ClassName ClassName
}

type Fact struct {
	Label   string
	Value   string
	Warning bool
}

type FactListProps struct {
	Key   any
	Facts []Fact
}

type Choice struct {
	Label   string
	Detail  string
	Icon    string
	Warning bool
}

type ChoiceListProps struct {
	Key       any
	Choices   []Choice
	Selected  *int
	ClassName ClassName
	OnSelect  func(index int)
}

type ReadoutProps struct {
	Key       any
	Value     string
	Unit      string
	ClassName ClassName
}

type NoticeProps struct {
	Key        any
	Icon       string
	Title      string
	Subtitle   string
	Severity   Severity
	ClassName  ClassName
	OnActivate func()
}

type CalendarEvent struct {
	Date   string
	Colors []Tone
}

type CalendarProps struct {
	Key       any
	Year      int
	Month     int
	Selected  string
	Today     string
	Events    []CalendarEvent
	ClassName ClassName
	OnMonth   func(year, month int)
	OnSelect  func(date string)
}

type DotsProps struct {
	Key       any
	Colors    []Tone
	Uniform   bool
	Size      int
	Max       int
	ClassName ClassName
}

type DropDownProps struct {
	Key       any
	Options   []string
	Selected  *int
	ClassName ClassName
	OnSelect  func(index int)
}

type ToggleButtonProps struct {
	Key       any
	Label     string
	Icon      string
	Active    bool
	ClassName ClassName
	OnToggle  func(active bool)
}

type CheckButtonProps struct {
	Key       any
	Label     string
	Active    bool
	ClassName ClassName
	OnToggle  func(active bool)
}

type RevealerProps struct {
	Key        any
	Revealed   bool
	Transition Transition
	ClassName  ClassName
}

type StackProps struct {
	Key       any
	Visible   int
	ClassName ClassName
}

type RangeBarProps struct {
	Key       any
	Min       float64
	Max       float64
	Low       float64
	High      float64
	Now       *float64
	ClassName ClassName
}

type BarsProps struct {
	Key           any
	Values        []*float64
	Maximum       *float64
	HighlightLast *bool
	Label         string
	Tooltips      []*string
	ClassName     ClassName
}

type SparklineProps struct {
	Key       any
	Values    []*float64
	Fill      bool
	ShowLast  bool
	Label     string
	ClassName ClassName
}

type SeparatorProps struct {
	Key         any
	Orientation Orientation
	ClassName   ClassName
}
