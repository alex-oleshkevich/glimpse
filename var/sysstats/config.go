package main

import (
	"bufio"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
)

const (
	configEnvVar  = "GLIMPSE_SYSSTATS_CONFIG"
	defaultConfig = "sysstats.toml"
)

type Config struct {
	Panel   PanelConfig
	Popover PopoverConfig
}

type PanelConfig struct {
	RefreshMS int
	Format    string
	Items     PanelItemsConfig
}

type PanelItemsConfig struct {
	CPU  MetricConfig
	RAM  MetricConfig
	Swap MetricConfig
}

type MetricConfig struct {
	Icon   string
	Label  string
	WarnAt int
}

type PopoverConfig struct {
	Sections PopoverSections
}

type PopoverSections struct {
	Network bool
	GPU     bool
	Disk    bool
	Temps   bool
	Uptime  bool
	Load    bool
	Memory  bool
}

type ConfigEnv struct {
	ExplicitPath  string
	LookupEnv     func(string) string
	WorkingDir    string
	XDGConfigHome string
}

func DefaultConfig() Config {
	return Config{
		Panel: PanelConfig{
			RefreshMS: 1000,
			Format:    "{cpu} {ram} {swap}",
			Items: PanelItemsConfig{
				CPU:  MetricConfig{Icon: "computer-symbolic", Label: "CPU"},
				RAM:  MetricConfig{Icon: "computer-symbolic", Label: "RAM", WarnAt: 90},
				Swap: MetricConfig{Icon: "drive-harddisk-symbolic", Label: "SWP", WarnAt: 80},
			},
		},
		Popover: PopoverConfig{
			Sections: PopoverSections{
				Network: true,
				GPU:     true,
				Disk:    true,
				Temps:   true,
				Uptime:  true,
				Load:    true,
				Memory:  true,
			},
		},
	}
}

func LoadConfig(env ConfigEnv) (Config, error) {
	cfg := DefaultConfig()

	path, found := resolveConfigPath(env)
	if !found {
		return cfg, nil
	}

	file, err := os.Open(path)
	if err != nil {
		return Config{}, err
	}
	defer file.Close()

	if err := decodeConfig(&cfg, file); err != nil {
		return Config{}, fmt.Errorf("decode %s: %w", path, err)
	}
	return cfg, nil
}

func resolveConfigPath(env ConfigEnv) (string, bool) {
	if env.ExplicitPath != "" {
		return env.ExplicitPath, true
	}

	lookupEnv := env.LookupEnv
	if lookupEnv == nil {
		lookupEnv = os.Getenv
	}
	if path := lookupEnv(configEnvVar); path != "" {
		return path, true
	}

	workingDir := env.WorkingDir
	if workingDir == "" {
		if cwd, err := os.Getwd(); err == nil {
			workingDir = cwd
		}
	}
	if workingDir != "" {
		candidate := filepath.Join(workingDir, defaultConfig)
		if fileExists(candidate) {
			return candidate, true
		}
	}

	xdg := env.XDGConfigHome
	if xdg == "" {
		xdg = lookupEnv("XDG_CONFIG_HOME")
	}
	if xdg != "" {
		candidate := filepath.Join(xdg, "glimpse", defaultConfig)
		if fileExists(candidate) {
			return candidate, true
		}
	}

	return "", false
}

func fileExists(path string) bool {
	info, err := os.Stat(path)
	return err == nil && !info.IsDir()
}

func decodeConfig(cfg *Config, file *os.File) error {
	scanner := bufio.NewScanner(file)
	section := ""

	for scanner.Scan() {
		line := strings.TrimSpace(stripComment(scanner.Text()))
		if line == "" {
			continue
		}
		if strings.HasPrefix(line, "[") && strings.HasSuffix(line, "]") {
			section = strings.TrimSpace(line[1 : len(line)-1])
			continue
		}

		key, rawValue, ok := strings.Cut(line, "=")
		if !ok {
			return fmt.Errorf("invalid line %q", line)
		}
		key = strings.TrimSpace(key)
		value, err := parseConfigValue(strings.TrimSpace(rawValue))
		if err != nil {
			return err
		}
		if err := applyConfigValue(cfg, section, key, value); err != nil {
			return err
		}
	}

	return scanner.Err()
}

func stripComment(line string) string {
	inString := false
	for idx, r := range line {
		switch r {
		case '"':
			inString = !inString
		case '#':
			if !inString {
				return line[:idx]
			}
		}
	}
	return line
}

func parseConfigValue(raw string) (any, error) {
	if len(raw) >= 2 && raw[0] == '"' && raw[len(raw)-1] == '"' {
		return strings.ReplaceAll(raw[1:len(raw)-1], `\"`, `"`), nil
	}
	if raw == "true" || raw == "false" {
		return raw == "true", nil
	}
	if value, err := strconv.Atoi(raw); err == nil {
		return value, nil
	}
	return nil, fmt.Errorf("unsupported value %q", raw)
}

func applyConfigValue(cfg *Config, section, key string, value any) error {
	switch section {
	case "panel":
		switch key {
		case "refresh_ms":
			cfg.Panel.RefreshMS = intValue(value)
		case "format":
			cfg.Panel.Format = stringValue(value)
		default:
			return nil
		}
	case "panel.items.cpu":
		return applyMetricConfig(&cfg.Panel.Items.CPU, key, value)
	case "panel.items.ram":
		return applyMetricConfig(&cfg.Panel.Items.RAM, key, value)
	case "panel.items.swap":
		return applyMetricConfig(&cfg.Panel.Items.Swap, key, value)
	case "popover.sections":
		return applySectionConfig(&cfg.Popover.Sections, key, value)
	default:
		return nil
	}
	return nil
}

func applyMetricConfig(metric *MetricConfig, key string, value any) error {
	switch key {
	case "icon":
		metric.Icon = stringValue(value)
	case "label":
		metric.Label = stringValue(value)
	case "warn_at":
		metric.WarnAt = intValue(value)
	default:
		return nil
	}
	return nil
}

func applySectionConfig(sections *PopoverSections, key string, value any) error {
	boolValue, ok := value.(bool)
	if !ok {
		return errors.New("section values must be booleans")
	}

	switch key {
	case "network":
		sections.Network = boolValue
	case "gpu":
		sections.GPU = boolValue
	case "disk":
		sections.Disk = boolValue
	case "temps":
		sections.Temps = boolValue
	case "uptime":
		sections.Uptime = boolValue
	case "load":
		sections.Load = boolValue
	case "memory":
		sections.Memory = boolValue
	}
	return nil
}

func stringValue(value any) string {
	if text, ok := value.(string); ok {
		return text
	}
	return ""
}

func intValue(value any) int {
	if number, ok := value.(int); ok {
		return number
	}
	return 0
}
