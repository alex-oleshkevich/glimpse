// Widget tree types. Each widget is a struct that implements Widget and
// MarshalJSON so it can be composed via struct literals:
//
//	Column{
//	    Spacing: 8,
//	    Children: []Widget{
//	        Hero{Title: "Counter"},
//	        Button{ID: "go", Label: "Go"},
//	    },
//	}
//
// Each MarshalJSON emits the canonical {"type": "<name>", "data": {...}}
// envelope. The inner data is the struct itself, marshaled via an
// unexported alias type so json does not recurse into our MarshalJSON.

package sdk

import (
	"encoding/json"
	"sort"
)

// Widget is any component that can appear in a popover tree. All concrete
// widget types implement this interface and json.Marshaler.
type Widget interface {
	isWidget()
}

func ensureSlice[T any](s []T) []T {
	if s == nil {
		return []T{}
	}
	return s
}

func Int(value int) *int {
	return &value
}

func envelope(typeName string, data any) ([]byte, error) {
	return json.Marshal(struct {
		Type string `json:"type"`
		Data any    `json:"data"`
	}{Type: typeName, Data: data})
}

// ---- Enums -----------------------------------------------------------------

type Align string
type Orientation string
type Variant string
type StatusVariant string
type ButtonVariant string
type PagerAppearance string
type ContentFit string
type LevelBarMode string
type Space string
type Color string
type Radius string
type FontSize string
type FontWeight string
type BorderWidth string

const (
	AlignFill     Align = "fill"
	AlignStart    Align = "start"
	AlignEnd      Align = "end"
	AlignCenter   Align = "center"
	AlignBaseline Align = "baseline"

	OrientationHorizontal Orientation = "horizontal"
	OrientationVertical   Orientation = "vertical"

	VariantNormal  Variant = "normal"
	VariantMuted   Variant = "muted"
	VariantAccent  Variant = "accent"
	VariantSuccess Variant = "success"
	VariantWarning Variant = "warning"
	VariantDanger  Variant = "danger"

	StatusVariantSuccess StatusVariant = "success"
	StatusVariantWarning StatusVariant = "warning"
	StatusVariantDanger  StatusVariant = "danger"

	ButtonVariantPrimary   ButtonVariant = "primary"
	ButtonVariantSecondary ButtonVariant = "secondary"
	ButtonVariantCompact   ButtonVariant = "compact"
	ButtonVariantFlat      ButtonVariant = "flat"
	ButtonVariantDanger    ButtonVariant = "danger"

	PagerAppearanceDots    PagerAppearance = "dots"
	PagerAppearanceNumbers PagerAppearance = "numbers"

	ContentFitFill      ContentFit = "fill"
	ContentFitContain   ContentFit = "contain"
	ContentFitCover     ContentFit = "cover"
	ContentFitScaleDown ContentFit = "scale_down"

	LevelBarModeContinuous LevelBarMode = "continuous"
	LevelBarModeDiscrete   LevelBarMode = "discrete"

	SpaceNone Space = "none"
	SpaceXXS  Space = "xxs"
	SpaceXS   Space = "xs"
	SpaceSM   Space = "sm"
	SpaceMD   Space = "md"
	SpaceLG   Space = "lg"

	ColorBG            Color = "bg"
	ColorFG            Color = "fg"
	ColorSurface       Color = "surface"
	ColorSurfaceRaised Color = "surface_raised"
	ColorBorder        Color = "border"
	ColorMutedFG       Color = "muted_fg"
	ColorAccent        Color = "accent"
	ColorAccentFG      Color = "accent_fg"
	ColorSuccess       Color = "success"
	ColorSuccessFG     Color = "success_fg"
	ColorWarning       Color = "warning"
	ColorWarningFG     Color = "warning_fg"
	ColorDanger        Color = "danger"
	ColorDangerFG      Color = "danger_fg"

	RadiusNone Radius = "none"
	RadiusSM   Radius = "sm"
	RadiusMD   Radius = "md"
	RadiusLG   Radius = "lg"
	RadiusPill Radius = "pill"

	FontSizeXXS  FontSize = "xxs"
	FontSizeXS   FontSize = "xs"
	FontSizeSM   FontSize = "sm"
	FontSizeMD   FontSize = "md"
	FontSizeBase FontSize = "base"
	FontSizeLG   FontSize = "lg"
	FontSizeXL   FontSize = "xl"

	FontWeightNormal   FontWeight = "normal"
	FontWeightMedium   FontWeight = "medium"
	FontWeightSemibold FontWeight = "semibold"
	FontWeightBold     FontWeight = "bold"

	BorderWidthNone   BorderWidth = "none"
	BorderWidthThin   BorderWidth = "thin"
	BorderWidthMedium BorderWidth = "medium"
	BorderWidthThick  BorderWidth = "thick"
)

// CommonProps are the shared layout / accessibility fields every widget
// accepts. Embed it as the first field of every widget struct.
type CommonProps struct {
	Visible    *bool             `json:"visible,omitempty"`
	HExpand    *bool             `json:"hexpand,omitempty"`
	VExpand    *bool             `json:"vexpand,omitempty"`
	HAlign     Align             `json:"halign,omitempty"`
	VAlign     Align             `json:"valign,omitempty"`
	Tooltip    string            `json:"tooltip,omitempty"`
	CssClasses []string          `json:"css_classes,omitempty"`
	Styles     map[string]string `json:"styles,omitempty"`
}

// ---- Display widgets -------------------------------------------------------

type Hero struct {
	CommonProps
	Title    string `json:"title"`
	Subtitle string `json:"subtitle"`
	Icon     string `json:"icon,omitempty"`
	ID       string `json:"id,omitempty"`
	SwitchOn *bool  `json:"switch,omitempty"`
}

func (Hero) isWidget() {}
func (h Hero) MarshalJSON() ([]byte, error) {
	type alias Hero
	return envelope("hero", alias(h))
}

type Icon struct {
	CommonProps
	Icon      string `json:"icon"`
	PixelSize *int   `json:"pixel_size,omitempty"`
}

func (Icon) isWidget() {}
func (w Icon) MarshalJSON() ([]byte, error) {
	type alias Icon
	return envelope("icon", alias(w))
}

type Picture struct {
	CommonProps
	Path       string     `json:"path"`
	ContentFit ContentFit `json:"content_fit,omitempty"`
}

func (Picture) isWidget() {}
func (w Picture) MarshalJSON() ([]byte, error) {
	type alias Picture
	return envelope("picture", alias(w))
}

type Label struct {
	CommonProps
	Text       string   `json:"text"`
	Wrap       bool     `json:"wrap,omitempty"`
	XAlign     *float32 `json:"xalign,omitempty"`
	Selectable bool     `json:"selectable,omitempty"`
	Variant    Variant  `json:"variant,omitempty"`
}

func (Label) isWidget() {}
func (l Label) MarshalJSON() ([]byte, error) {
	type alias Label
	return envelope("label", alias(l))
}

type Badge struct {
	CommonProps
	Label   string  `json:"label"`
	Variant Variant `json:"variant,omitempty"`
}

func (Badge) isWidget() {}
func (b Badge) MarshalJSON() ([]byte, error) {
	type alias Badge
	return envelope("badge", alias(b))
}

type StatusDot struct {
	CommonProps
	Variant StatusVariant `json:"variant,omitempty"`
}

func (StatusDot) isWidget() {}
func (s StatusDot) MarshalJSON() ([]byte, error) {
	type alias StatusDot
	return envelope("status", alias(s))
}

type PagerItem struct {
	CommonProps
	ID         string          `json:"id,omitempty"`
	Appearance PagerAppearance `json:"appearance"`
	Label      string          `json:"label"`
	Active     bool            `json:"active"`
	Inactive   bool            `json:"inactive"`
	Occupied   bool            `json:"occupied"`
	Urgent     bool            `json:"urgent"`
}

func (PagerItem) isWidget() {}
func (p PagerItem) MarshalJSON() ([]byte, error) {
	return envelope("pager_item", pagerItemData(p))
}

type PagerStrip struct {
	CommonProps
	ID    string      `json:"id,omitempty"`
	Items []PagerItem `json:"items"`
}

func (PagerStrip) isWidget() {}
func (p PagerStrip) MarshalJSON() ([]byte, error) {
	items := make([]pagerItemAlias, 0, len(p.Items))
	for _, item := range p.Items {
		items = append(items, pagerItemData(item))
	}
	return envelope("pager_strip", struct {
		CommonProps
		ID    string           `json:"id,omitempty"`
		Items []pagerItemAlias `json:"items"`
	}{
		CommonProps: p.CommonProps,
		ID:          p.ID,
		Items:       ensureSlice(items),
	})
}

type pagerItemAlias PagerItem

func pagerItemData(item PagerItem) pagerItemAlias {
	if item.Appearance == "" {
		item.Appearance = PagerAppearanceDots
	}
	return pagerItemAlias(item)
}

type Spinner struct {
	CommonProps
	Spinning bool `json:"spinning"`
}

func (Spinner) isWidget() {}
func (s Spinner) MarshalJSON() ([]byte, error) {
	type alias Spinner
	return envelope("spinner", alias(s))
}

type Separator struct {
	CommonProps
	Orientation Orientation `json:"orientation,omitempty"`
}

func (Separator) isWidget() {}
func (s Separator) MarshalJSON() ([]byte, error) {
	type alias Separator
	return envelope("separator", alias(s))
}

type EmptyState struct {
	CommonProps
	Title    string `json:"title"`
	Subtitle string `json:"subtitle"`
}

func (EmptyState) isWidget() {}
func (e EmptyState) MarshalJSON() ([]byte, error) {
	type alias EmptyState
	return envelope("empty_state", alias(e))
}

type Progress struct {
	CommonProps
	Value    float64 `json:"value"`
	Max      float64 `json:"max"`
	ShowText bool    `json:"show_text,omitempty"`
	Text     string  `json:"text,omitempty"`
}

func (Progress) isWidget() {}
func (p Progress) MarshalJSON() ([]byte, error) {
	type alias Progress
	return envelope("progress", alias(p))
}

type LevelBar struct {
	CommonProps
	Value float64      `json:"value"`
	Min   float64      `json:"min"`
	Max   float64      `json:"max"`
	Mode  LevelBarMode `json:"mode"`
}

func (LevelBar) isWidget() {}
func (l LevelBar) MarshalJSON() ([]byte, error) {
	type alias LevelBar
	out := l
	if out.Max == 0 {
		out.Max = 1
	}
	if out.Mode == "" {
		out.Mode = LevelBarModeContinuous
	}
	return envelope("level_bar", alias(out))
}

type Meter struct {
	CommonProps
	ID          string  `json:"id,omitempty"`
	Icon        string  `json:"icon,omitempty"`
	Label       string  `json:"label"`
	Value       float64 `json:"value"`
	Min         float64 `json:"min"`
	Max         float64 `json:"max"`
	Step        float64 `json:"step"`
	Text        string  `json:"text,omitempty"`
	Interactive bool    `json:"interactive"`
}

func (Meter) isWidget() {}
func (m Meter) MarshalJSON() ([]byte, error) {
	type alias Meter
	return envelope("meter", alias(m))
}

type Copyable struct {
	CommonProps
	Label string `json:"label,omitempty"`
	Value string `json:"value"`
}

func (Copyable) isWidget() {}
func (c Copyable) MarshalJSON() ([]byte, error) {
	type alias Copyable
	return envelope("copyable", alias(c))
}

// ---- Interactive widgets ---------------------------------------------------

type Button struct {
	ID string `json:"id"`
	CommonProps
	Label   string        `json:"label,omitempty"`
	Icon    string        `json:"icon,omitempty"`
	Enabled *bool         `json:"enabled,omitempty"`
	Variant ButtonVariant `json:"variant,omitempty"`
}

func (Button) isWidget() {}
func (b Button) MarshalJSON() ([]byte, error) {
	type alias Button
	return envelope("button", alias(b))
}

type LinkButton struct {
	CommonProps
	URI   string `json:"uri"`
	Label string `json:"label,omitempty"`
}

func (LinkButton) isWidget() {}
func (l LinkButton) MarshalJSON() ([]byte, error) {
	type alias LinkButton
	return envelope("link_button", alias(l))
}

type Expander struct {
	CommonProps
	Label    string `json:"label"`
	Expanded bool   `json:"expanded"`
	Child    Widget `json:"child"`
}

func (Expander) isWidget() {}
func (e Expander) MarshalJSON() ([]byte, error) {
	type alias Expander
	return envelope("expander", alias(e))
}

type Switch struct {
	ID string `json:"id"`
	CommonProps
	Label  string `json:"label,omitempty"`
	Active bool   `json:"active"`
}

func (Switch) isWidget() {}
func (s Switch) MarshalJSON() ([]byte, error) {
	type alias Switch
	return envelope("switch", alias(s))
}

type ToggleButton struct {
	ID string `json:"id"`
	CommonProps
	Label  string `json:"label,omitempty"`
	Icon   string `json:"icon,omitempty"`
	Active bool   `json:"active"`
}

func (ToggleButton) isWidget() {}
func (t ToggleButton) MarshalJSON() ([]byte, error) {
	type alias ToggleButton
	return envelope("toggle_button", alias(t))
}

type Checkbox struct {
	ID string `json:"id"`
	CommonProps
	Label  string `json:"label,omitempty"`
	Active bool   `json:"active"`
}

func (Checkbox) isWidget() {}
func (c Checkbox) MarshalJSON() ([]byte, error) {
	type alias Checkbox
	return envelope("checkbox", alias(c))
}

type Slider struct {
	ID string `json:"id"`
	CommonProps
	Min         float64     `json:"min"`
	Max         float64     `json:"max"`
	Step        float64     `json:"step"`
	Value       float64     `json:"value"`
	Orientation Orientation `json:"orientation,omitempty"`
	DrawValue   bool        `json:"draw_value,omitempty"`
}

func (Slider) isWidget() {}
func (s Slider) MarshalJSON() ([]byte, error) {
	type alias Slider
	return envelope("slider", alias(s))
}

type Select struct {
	ID string `json:"id"`
	CommonProps
	Items    []map[string]string `json:"items"`
	Selected *uint32             `json:"selected,omitempty"`
}

func (Select) isWidget() {}
func (d Select) MarshalJSON() ([]byte, error) {
	type alias Select
	out := d
	if out.Items == nil {
		out.Items = []map[string]string{}
	}
	return envelope("select", alias(out))
}

// ---- Layouts ---------------------------------------------------------------

type Row struct {
	CommonProps
	Spacing    int      `json:"spacing"`
	SpacingSet bool     `json:"-"`
	Children   []Widget `json:"children"`
}

func (Row) isWidget() {}
func (r Row) MarshalJSON() ([]byte, error) {
	type alias Row
	out := r
	if !out.SpacingSet && out.Spacing == 0 {
		out.Spacing = 4
	}
	out.Children = ensureSlice(out.Children)
	return envelope("row", alias(out))
}

type Column struct {
	CommonProps
	Spacing    int      `json:"spacing"`
	SpacingSet bool     `json:"-"`
	Children   []Widget `json:"children"`
}

func (Column) isWidget() {}
func (c Column) MarshalJSON() ([]byte, error) {
	type alias Column
	out := c
	if !out.SpacingSet && out.Spacing == 0 {
		out.Spacing = 4
	}
	out.Children = ensureSlice(out.Children)
	return envelope("column", alias(out))
}

type GridChild struct {
	Row    int    `json:"row"`
	Column int    `json:"column"`
	Width  int    `json:"width"`
	Height int    `json:"height"`
	Child  Widget `json:"child"`
}

type Grid struct {
	CommonProps
	Children         []GridChild `json:"children"`
	RowSpacing       int         `json:"row_spacing"`
	RowSpacingSet    bool        `json:"-"`
	ColumnSpacing    int         `json:"column_spacing"`
	ColumnSpacingSet bool        `json:"-"`
}

func (Grid) isWidget() {}
func (g Grid) MarshalJSON() ([]byte, error) {
	type alias Grid
	out := g
	if !out.RowSpacingSet && out.RowSpacing == 0 {
		out.RowSpacing = 4
	}
	if !out.ColumnSpacingSet && out.ColumnSpacing == 0 {
		out.ColumnSpacing = 4
	}
	out.Children = ensureSlice(out.Children)
	return envelope("grid", alias(out))
}

type Card struct {
	CommonProps
	Child Widget `json:"child,omitempty"`
}

func (Card) isWidget() {}
func (c Card) MarshalJSON() ([]byte, error) {
	type alias Card
	return envelope("card", alias(c))
}

type Container struct {
	CommonProps
	Child         Widget      `json:"child,omitempty"`
	Width         *int        `json:"width,omitempty"`
	Height        *int        `json:"height,omitempty"`
	MinWidth      *int        `json:"min_width,omitempty"`
	MinHeight     *int        `json:"min_height,omitempty"`
	Margin        Space       `json:"margin,omitempty"`
	MarginTop     Space       `json:"margin_top,omitempty"`
	MarginRight   Space       `json:"margin_right,omitempty"`
	MarginBottom  Space       `json:"margin_bottom,omitempty"`
	MarginLeft    Space       `json:"margin_left,omitempty"`
	Padding       Space       `json:"padding,omitempty"`
	PaddingTop    Space       `json:"padding_top,omitempty"`
	PaddingRight  Space       `json:"padding_right,omitempty"`
	PaddingBottom Space       `json:"padding_bottom,omitempty"`
	PaddingLeft   Space       `json:"padding_left,omitempty"`
	Background    Color       `json:"background,omitempty"`
	Color         Color       `json:"color,omitempty"`
	BorderRadius  Radius      `json:"border_radius,omitempty"`
	BorderWidth   BorderWidth `json:"border_width,omitempty"`
	BorderColor   Color       `json:"border_color,omitempty"`
	FontSize      FontSize    `json:"font_size,omitempty"`
	FontWeight    FontWeight  `json:"font_weight,omitempty"`
}

func (Container) isWidget() {}
func (c Container) MarshalJSON() ([]byte, error) {
	type alias Container
	return envelope("container", alias(c))
}

type Scroll struct {
	CommonProps
	Child Widget `json:"child"`
}

func (Scroll) isWidget() {}
func (s Scroll) MarshalJSON() ([]byte, error) {
	type alias Scroll
	return envelope("scroll", alias(s))
}

type Properties map[string]string

type PropertyList struct {
	CommonProps
	Title string     `json:"title,omitempty"`
	Rows  Properties `json:"rows"`
}

func (PropertyList) isWidget() {}
func (p PropertyList) MarshalJSON() ([]byte, error) {
	type row struct {
		Key   string `json:"key"`
		Value string `json:"value"`
	}
	type data struct {
		CommonProps
		Title string `json:"title,omitempty"`
		Rows  []row  `json:"rows"`
	}

	keys := make([]string, 0, len(p.Rows))
	for key := range p.Rows {
		keys = append(keys, key)
	}
	sort.Strings(keys)

	rows := make([]row, 0, len(keys))
	for _, key := range keys {
		rows = append(rows, row{Key: key, Value: p.Rows[key]})
	}

	return envelope("property_list", data{
		CommonProps: p.CommonProps,
		Title:       p.Title,
		Rows:        rows,
	})
}

type Item struct {
	CommonProps
	Icon     string `json:"-"`
	Left     Widget `json:"left,omitempty"`
	Label    string `json:"label"`
	Sublabel string `json:"sublabel,omitempty"`
	Right    Widget `json:"right,omitempty"`
}

func (Item) isWidget() {}
func (i Item) MarshalJSON() ([]byte, error) {
	if i.Left == nil && i.Icon != "" {
		size := 16
		i.Left = Icon{Icon: i.Icon, PixelSize: &size}
	}
	type alias Item
	return envelope("item", alias(i))
}

type ActionItem struct {
	ID string `json:"id"`
	CommonProps
	Icon     string `json:"-"`
	Left     Widget `json:"left,omitempty"`
	Label    string `json:"label"`
	Sublabel string `json:"sublabel,omitempty"`
	Right    Widget `json:"right,omitempty"`
	Enabled  *bool  `json:"enabled,omitempty"`
}

func (ActionItem) isWidget() {}
func (i ActionItem) MarshalJSON() ([]byte, error) {
	if i.Left == nil && i.Icon != "" {
		size := 16
		i.Left = Icon{Icon: i.Icon, PixelSize: &size}
	}
	type alias ActionItem
	return envelope("action_item", alias(i))
}

type PopoverSize string

const (
	PopoverSizeSmall  PopoverSize = "small"
	PopoverSizeMedium PopoverSize = "medium"
	PopoverSizeLarge  PopoverSize = "large"
	PopoverSizeXLarge PopoverSize = "xlarge"
)

type PopoverScaffold struct {
	Hero Widget      `json:"hero,omitempty"`
	Body Widget      `json:"body"`
	Size PopoverSize `json:"size,omitempty"`
}

func (PopoverScaffold) isWidget() {}
func (w PopoverScaffold) MarshalJSON() ([]byte, error) {
	type alias PopoverScaffold
	return envelope("popover_scaffold", alias(w))
}
