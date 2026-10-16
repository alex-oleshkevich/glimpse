package sdk

import (
	"bufio"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"net"
	"os"
	"strings"
	"syscall"
)

func Run(app App) {
	code := run(app)
	os.Exit(code)
}

func run(app App) (code int) {
	defer func() {
		if r := recover(); r != nil {
			fmt.Fprintln(os.Stderr, r)
			switch r.(type) {
			case limitError, renderError:
				code = 70
			default:
				code = 1
			}
		}
	}()

	path := os.Getenv("GLIMPSE_APPLET_SOCKET")
	var conn net.Conn
	var reader *bufio.Reader
	var writeLine func(string)

	if path != "" {
		dialed, err := net.Dial("unix", path)
		if err != nil {
			if errors.Is(err, fs.ErrNotExist) || errors.Is(err, syscall.ECONNREFUSED) {
				return 0
			}
			fmt.Fprintln(os.Stderr, err)
			return 1
		}
		conn = dialed
		defer conn.Close()

		reader = bufio.NewReader(conn)
		writeLine = func(line string) { _, _ = conn.Write([]byte(line + "\n")) }
	} else {
		stdout := os.Stdout
		os.Stdout = os.Stderr
		reader = bufio.NewReader(os.Stdin)
		writeLine = func(line string) { _, _ = stdout.WriteString(line + "\n") }
	}

	lines := make(chan string, 16)
	go func() {
		for {
			line, err := reader.ReadString('\n')
			if line != "" {
				lines <- line
			}
			if err != nil {
				close(lines)
				return
			}
		}
	}()

	wake := make(chan struct{}, 1)
	session := NewSession(app, writeLine)
	session.OnWake = func() {
		select {
		case wake <- struct{}{}:
		default:
		}
	}
	session.Send(map[string]any{"t": "hello", "v": 1})

	closed := false
	for {
		var batch []string
		select {
		case line, ok := <-lines:
			if !ok {
				return 0
			}
			batch = append(batch, line)
		case <-wake:
			session.Dirty = true
		}
	drain:
		for {
			select {
			case line, ok := <-lines:
				if !ok {
					closed = true
					break drain
				}
				batch = append(batch, line)
			case <-wake:
				session.Dirty = true
			default:
				break drain
			}
		}

		for _, line := range batch {
			if strings.TrimSpace(line) == "" {
				continue
			}

			var msg map[string]any
			if err := json.Unmarshal([]byte(line), &msg); err != nil {
				fmt.Fprintln(os.Stderr, err)
				continue
			}
			if _, ok := msg["t"].(string); ok {
				session.Handle(msg)
			}
		}
		session.Flush()
		if closed {
			return 0
		}
	}
}
