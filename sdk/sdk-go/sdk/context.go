package sdk

import (
	"fmt"
	"os"
	"os/exec"
	"strings"
)

type AppletEntry struct {
	ID   string
	Name string
	Icon string
}

type Context struct {
	session     *Session
	Name        string
	StateDir    string
	Options     map[string]any
	Placement   map[string]any
	PopoverOpen bool
	Entry       *AppletEntry
}

func (c *Context) send(msg map[string]any) {
	defer func() {
		if r := recover(); r != nil {
			fmt.Fprintln(os.Stderr, r)
			os.Exit(exitCode(r))
		}
	}()
	c.session.Send(msg)
}

func (c *Context) Wake() { c.session.Wake() }

func spawn(program string, args []string, stdin string, hasStdin bool) {
	cmd := exec.Command(program, args...)
	if hasStdin {
		cmd.Stdin = strings.NewReader(stdin)
	}

	if err := cmd.Start(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		return
	}

	go func() { _ = cmd.Wait() }()
}

func notifyArgv(name string, entry *AppletEntry, n Notification) []string {
	app := name
	if entry != nil && entry.Name != "" {
		app = entry.Name
	}

	urgency := string(n.Urgency)
	if urgency != "low" && urgency != "normal" && urgency != "critical" {
		urgency = "normal"
	}

	args := []string{"notify-send", "--app-name=" + app, "--urgency=" + urgency}

	icon := n.Icon
	if icon == "" && entry != nil {
		icon = entry.Icon
	}
	if icon != "" {
		args = append(args, "--icon="+icon)
	}

	if entry != nil {
		if id := strings.TrimSuffix(entry.ID, ".desktop"); id != "" {
			args = append(args, "--hint=string:desktop-entry:"+id)
		}
	}

	args = append(args, "--", n.Summary)
	if n.Body != "" {
		args = append(args, n.Body)
	}

	return args
}

func (c *Context) Notify(n Notification) {
	argv := notifyArgv(c.Name, c.Entry, n)
	spawn(argv[0], argv[1:], "", false)
}

func (c *Context) Copy(text string) { spawn("wl-copy", nil, text, true) }

func (c *Context) OpenURI(uri string) {
	c.send(map[string]any{"t": "open-uri", "uri": uri})
}

func (c *Context) Launch(id string) { c.send(map[string]any{"t": "launch", "id": id}) }

func (c *Context) ClosePopover() { c.send(map[string]any{"t": "close-popover"}) }
