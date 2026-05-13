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

import "encoding/json"

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
)

// CommonProps are the shared layout / accessibility fields every widget
// accepts. Embed it as the first field of every widget struct.
type CommonProps struct {
	ID      string  `json:"id,omitempty"`
	Visible *bool   `json:"visible,omitempty"`
	HExpand *bool   `json:"hexpand,omitempty"`
	VExpand *bool   `json:"vexpand,omitempty"`
	HAlign  Align   `json:"halign,omitempty"`
	VAlign  Align   `json:"valign,omitempty"`
	Tooltip string  `json:"tooltip,omitempty"`
	Variant Variant `json:"variant,omitempty"`
}

// ---- Display widgets -------------------------------------------------------

type Hero struct {
	CommonProps
	Title    string `json:"title"`
	Subtitle string `json:"subtitle"`
	Icon     *Icon  `json:"icon,omitempty"`
}

func (Hero) isWidget() {}
func (h Hero) MarshalJSON() ([]byte, error) {
	type alias Hero
	return envelope("hero", alias(h))
}

type IconWidget struct {
	CommonProps
	Icon      *Icon `json:"icon"`
	PixelSize *int  `json:"pixel_size,omitempty"`
}

func (IconWidget) isWidget() {}
func (w IconWidget) MarshalJSON() ([]byte, error) {
	type alias IconWidget
	return envelope("icon", alias(w))
}

type Image struct {
	CommonProps
	Icon      *Icon `json:"icon"`
	PixelSize *int  `json:"pixel_size,omitempty"`
}

func (Image) isWidget() {}
func (w Image) MarshalJSON() ([]byte, error) {
	type alias Image
	return envelope("image", alias(w))
}

type Label struct {
	CommonProps
	Text       string   `json:"text"`
	Wrap       bool     `json:"wrap,omitempty"`
	XAlign     *float32 `json:"xalign,omitempty"`
	Selectable bool     `json:"selectable,omitempty"`
}

func (Label) isWidget() {}
func (l Label) MarshalJSON() ([]byte, error) {
	type alias Label
	return envelope("label", alias(l))
}

type Badge struct {
	CommonProps
	Label string `json:"label"`
}

func (Badge) isWidget() {}
func (b Badge) MarshalJSON() ([]byte, error) {
	type alias Badge
	return envelope("badge", alias(b))
}

type StatusDot struct {
	CommonProps
}

func (StatusDot) isWidget() {}
func (s StatusDot) MarshalJSON() ([]byte, error) {
	type alias StatusDot
	return envelope("status", alias(s))
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

type Meter struct {
	CommonProps
	Icon        *Icon   `json:"icon,omitempty"`
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

type ToastAction struct {
	ID    string `json:"id"`
	Label string `json:"label"`
}

type Toast struct {
	CommonProps
	Icon    *Icon        `json:"icon,omitempty"`
	Title   string       `json:"title"`
	Message string       `json:"message"`
	Action  *ToastAction `json:"action,omitempty"`
}

func (Toast) isWidget() {}
func (t Toast) MarshalJSON() ([]byte, error) {
	type alias Toast
	return envelope("toast", alias(t))
}

// ---- Interactive widgets ---------------------------------------------------

type Button struct {
	CommonProps
	Label string `json:"label,omitempty"`
	Icon  *Icon  `json:"icon,omitempty"`
	Child Widget `json:"child,omitempty"`
}

func (Button) isWidget() {}
func (b Button) MarshalJSON() ([]byte, error) {
	type alias Button
	return envelope("button", alias(b))
}

type Switch struct {
	CommonProps
	Label  string `json:"label,omitempty"`
	Active bool   `json:"active"`
}

func (Switch) isWidget() {}
func (s Switch) MarshalJSON() ([]byte, error) {
	type alias Switch
	return envelope("switch", alias(s))
}

type Checkbox struct {
	CommonProps
	Label  string `json:"label,omitempty"`
	Active bool   `json:"active"`
}

func (Checkbox) isWidget() {}
func (c Checkbox) MarshalJSON() ([]byte, error) {
	type alias Checkbox
	return envelope("checkbox", alias(c))
}

type Scale struct {
	CommonProps
	Min         float64     `json:"min"`
	Max         float64     `json:"max"`
	Step        float64     `json:"step"`
	Value       float64     `json:"value"`
	Orientation Orientation `json:"orientation,omitempty"`
	DrawValue   bool        `json:"draw_value,omitempty"`
}

func (Scale) isWidget() {}
func (s Scale) MarshalJSON() ([]byte, error) {
	type alias Scale
	return envelope("scale", alias(s))
}

type DropdownItem struct {
	ID    string `json:"id"`
	Label string `json:"label"`
}

type Dropdown struct {
	CommonProps
	Items    []DropdownItem `json:"items"`
	Selected *uint32        `json:"selected,omitempty"`
}

func (Dropdown) isWidget() {}
func (d Dropdown) MarshalJSON() ([]byte, error) {
	type alias Dropdown
	out := d
	out.Items = ensureSlice(out.Items)
	return envelope("dropdown", alias(out))
}

// ---- Layouts ---------------------------------------------------------------

type Box struct {
	CommonProps
	Orientation Orientation `json:"orientation"`
	Spacing     int         `json:"spacing"`
	Children    []Widget    `json:"children"`
}

func (Box) isWidget() {}
func (b Box) MarshalJSON() ([]byte, error) {
	type alias Box
	out := b
	out.Children = ensureSlice(out.Children)
	return envelope("box", alias(out))
}

type Row struct {
	CommonProps
	Spacing  int      `json:"spacing"`
	Children []Widget `json:"children"`
}

func (Row) isWidget() {}
func (r Row) MarshalJSON() ([]byte, error) {
	type alias Row
	out := r
	out.Children = ensureSlice(out.Children)
	return envelope("row", alias(out))
}

type Column struct {
	CommonProps
	Spacing  int      `json:"spacing"`
	Children []Widget `json:"children"`
}

func (Column) isWidget() {}
func (c Column) MarshalJSON() ([]byte, error) {
	type alias Column
	out := c
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
	Children      []GridChild `json:"children"`
	RowSpacing    int         `json:"row_spacing"`
	ColumnSpacing int         `json:"column_spacing"`
}

func (Grid) isWidget() {}
func (g Grid) MarshalJSON() ([]byte, error) {
	type alias Grid
	out := g
	out.Children = ensureSlice(out.Children)
	return envelope("grid", alias(out))
}

type Card struct {
	CommonProps
	Children []Widget `json:"children"`
}

func (Card) isWidget() {}
func (c Card) MarshalJSON() ([]byte, error) {
	type alias Card
	out := c
	out.Children = ensureSlice(out.Children)
	return envelope("card", alias(out))
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

// ---- Group widgets ---------------------------------------------------------

type Header struct {
	Title    string `json:"title"`
	Subtitle string `json:"subtitle,omitempty"`
}

type Section struct {
	CommonProps
	Header *Header  `json:"header,omitempty"`
	Body   []Widget `json:"body"`
}

func (Section) isWidget() {}
func (s Section) MarshalJSON() ([]byte, error) {
	type alias Section
	out := s
	out.Body = ensureSlice(out.Body)
	return envelope("section", alias(out))
}

type Collapsible struct {
	CommonProps
	Header   *Header  `json:"header,omitempty"`
	Expanded bool     `json:"expanded"`
	Body     []Widget `json:"body"`
}

func (Collapsible) isWidget() {}
func (c Collapsible) MarshalJSON() ([]byte, error) {
	type alias Collapsible
	out := c
	out.Body = ensureSlice(out.Body)
	return envelope("collapsible", alias(out))
}

// ---- List rows -------------------------------------------------------------

type Item struct {
	CommonProps
	Left      Widget     `json:"left,omitempty"`
	Label     string     `json:"label"`
	Right     Widget     `json:"right,omitempty"`
	Clickable bool       `json:"clickable"`
	Menu      []MenuItem `json:"menu"`
}

func (Item) isWidget() {}
func (i Item) MarshalJSON() ([]byte, error) {
	type alias Item
	out := i
	out.Menu = ensureSlice(out.Menu)
	return envelope("item", alias(out))
}

type CollapsibleItem struct {
	CommonProps
	Left     Widget   `json:"left,omitempty"`
	Label    string   `json:"label"`
	Right    Widget   `json:"right,omitempty"`
	Expanded bool     `json:"expanded"`
	Body     []Widget `json:"body"`
}

func (CollapsibleItem) isWidget() {}
func (c CollapsibleItem) MarshalJSON() ([]byte, error) {
	type alias CollapsibleItem
	out := c
	out.Body = ensureSlice(out.Body)
	return envelope("collapsible_item", alias(out))
}

type ActionRow struct {
	CommonProps
	Title    string `json:"title"`
	Subtitle string `json:"subtitle"`
	Meta     string `json:"meta"`
	Icon     *Icon  `json:"icon,omitempty"`
}

func (ActionRow) isWidget() {}
func (a ActionRow) MarshalJSON() ([]byte, error) {
	type alias ActionRow
	return envelope("action_row", alias(a))
}

type ActionMenuItem struct {
	ID         string `json:"id"`
	Label      string `json:"label"`
	Icon       *Icon  `json:"icon,omitempty"`
	Visible    *bool  `json:"visible,omitempty"`
	Checked    *bool  `json:"checked,omitempty"`
	Selectable *bool  `json:"selectable,omitempty"`
}

type ActionMenu struct {
	CommonProps
	Header string           `json:"header,omitempty"`
	Items  []ActionMenuItem `json:"items"`
}

func (ActionMenu) isWidget() {}
func (a ActionMenu) MarshalJSON() ([]byte, error) {
	type alias ActionMenu
	out := a
	out.Items = ensureSlice(out.Items)
	return envelope("action_menu", alias(out))
}

type DetailGridItem struct {
	Key   string `json:"key"`
	Value string `json:"value"`
}

type DetailGrid struct {
	CommonProps
	Rows []DetailGridItem `json:"rows"`
}

func (DetailGrid) isWidget() {}
func (d DetailGrid) MarshalJSON() ([]byte, error) {
	type alias DetailGrid
	out := d
	out.Rows = ensureSlice(out.Rows)
	return envelope("detail_grid", alias(out))
}
