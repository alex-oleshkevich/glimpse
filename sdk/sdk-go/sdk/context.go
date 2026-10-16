package sdk

import (
	"math"
	"time"
)

func PositionNow(player map[string]any, nowMs float64) float64 {
	position, _ := player["positionUs"].(float64)
	if player["playback"] != "playing" {
		return position
	}

	if nowMs == 0 {
		nowMs = float64(time.Now().UnixMilli())
	}

	at, _ := player["positionAt"].(float64)
	rate, ok := player["rate"].(float64)
	if !ok {
		rate = 1
	}

	out := position + (nowMs-at)*1000*rate
	if length, ok := player["lengthUs"].(float64); ok && length > 0 && out > length {
		return length
	}

	return out
}

type Context struct {
	session     *Session
	Name        string
	StateDir    string
	Options     map[string]any
	Placement   map[string]any
	PopoverOpen bool
	Services    map[string]any
}

func (c *Context) Mpris() []map[string]any {
	value, ok := c.Services["mpris"].(map[string]any)
	if !ok {
		return nil
	}

	raw, _ := value["players"].([]any)
	players := make([]map[string]any, 0, len(raw))
	for _, item := range raw {
		if player, ok := item.(map[string]any); ok {
			players = append(players, player)
		}
	}

	return players
}

func (c *Context) Compositor() map[string]any {
	value, _ := c.Services["compositor"].(map[string]any)
	return value
}

func (c *Context) Wake() { c.session.Wake() }

func (c *Context) Notify(n Notification) {
	msg := map[string]any{"t": "notify", "summary": n.Summary}
	if n.Body != "" {
		msg["body"] = n.Body
	}

	if n.Icon != "" {
		msg["icon"] = n.Icon
	}

	if n.Urgency != "" {
		msg["urgency"] = string(n.Urgency)
	}

	c.session.Send(msg)
}

func (c *Context) Copy(text string) { c.session.Send(map[string]any{"t": "copy", "text": text}) }

func (c *Context) OpenURI(uri string) {
	c.session.Send(map[string]any{"t": "open-uri", "uri": uri})
}

func (c *Context) Launch(id string) { c.session.Send(map[string]any{"t": "launch", "id": id}) }

func (c *Context) ClosePopover() { c.session.Send(map[string]any{"t": "close-popover"}) }

func (c *Context) MprisControl(player, action string) {
	c.session.Send(map[string]any{"t": "mpris", "player": player, "op": "control", "action": action})
}

func (c *Context) MprisSeek(player string, offsetUs float64) {
	c.session.Send(map[string]any{"t": "mpris", "player": player, "op": "seek", "offset": math.Round(offsetUs)})
}

func (c *Context) MprisSetPosition(player string, positionUs float64) {
	c.session.Send(map[string]any{"t": "mpris", "player": player, "op": "set-position", "position": math.Round(positionUs)})
}

func (c *Context) FocusWorkspace(target any) {
	c.session.Send(map[string]any{"t": "compositor", "op": "focus-workspace", "target": target})
}

func (c *Context) FocusWindow(target any) {
	c.session.Send(map[string]any{"t": "compositor", "op": "focus-window", "target": target})
}

func (c *Context) MoveWindow(window int, target any) {
	c.session.Send(map[string]any{"t": "compositor", "op": "move-window", "window": window, "target": target})
}

func (c *Context) RenameWorkspace(id int, name *string) {
	c.session.Send(map[string]any{"t": "compositor", "op": "rename-workspace", "id": id, "name": name})
}

func (c *Context) CloseWindow(id int) {
	c.session.Send(map[string]any{"t": "compositor", "op": "close-window", "id": id})
}
