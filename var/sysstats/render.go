package main

import (
	"fmt"
	"math"
	"strconv"
	"strings"
	"time"

	sdk "github.com/glimpse-project/custom-applet-sdk-go/sdk"
)

func buildStatusItems(config Config, snapshot Snapshot) []sdk.StatusItem {
	items := []sdk.StatusItem{
		buildPanelMetric("cpu", config.Panel.Items.CPU, snapshot.CPUPercent, false),
		buildPanelMetric("ram", config.Panel.Items.RAM, snapshot.Memory.UsedPercent, thresholdExceeded(snapshot.Memory.UsedPercent, config.Panel.Items.RAM.WarnAt)),
		buildPanelMetric("swap", config.Panel.Items.Swap, snapshot.Swap.UsedPercent, thresholdExceeded(snapshot.Swap.UsedPercent, config.Panel.Items.Swap.WarnAt)),
	}
	return items
}

func buildPanelMetric(id string, metric MetricConfig, percent float64, warn bool) sdk.StatusItem {
	text := strings.TrimSpace(metric.Label + " " + strconv.Itoa(int(math.Round(percent))) + "%")
	if warn {
		text += "!"
	}
	return sdk.StatusItem{
		ID:    id,
		Icon:  iconName(metric.Icon),
		Label: text,
	}
}

func buildPopoverTree(config Config, snapshot Snapshot) *sdk.TreeNode {
	children := []sdk.TreeNode{
		buildHeroNode(snapshot),
		buildDetailSection(
			"CPU & Memory",
			appendRows(
				row("CPU usage", fmt.Sprintf("%d%%", rounded(snapshot.CPUPercent))),
				conditionalRow(config.Popover.Sections.Load, "Load average", fmt.Sprintf("%.2f %.2f %.2f", snapshot.Load.One, snapshot.Load.Five, snapshot.Load.Fifteen)),
				conditionalRow(config.Popover.Sections.Memory, "RAM used", fmt.Sprintf("%s / %s", formatBytes(snapshot.Memory.UsedBytes), formatBytes(snapshot.Memory.TotalBytes))),
				conditionalRow(config.Popover.Sections.Memory, "Swap used", fmt.Sprintf("%s / %s", formatBytes(snapshot.Swap.UsedBytes), formatBytes(snapshot.Swap.TotalBytes))),
				optionalWarningLevels(config),
			),
		),
	}

	if config.Popover.Sections.Network {
		rows := appendRows(
			optionalInterfaceRow(snapshot.Network),
			optionalRow("IPv4", snapshot.Network.IPv4),
			optionalRow("Throughput", formatThroughput(snapshot.Network)),
		)
		children = append(children, buildSectionWithFallback("Network", rows, "Network unavailable", "No active interface detected."))
	}

	if config.Popover.Sections.GPU && len(snapshot.GPUs) > 0 {
		rows := make([]sectionRow, 0, len(snapshot.GPUs))
		for _, gpu := range snapshot.GPUs {
			rows = append(rows, row(gpu.Label(), formatGPU(gpu)))
		}
		children = append(children, buildDetailSection("GPU", rows))
	}

	if config.Popover.Sections.Disk && len(snapshot.Disks) > 0 {
		rows := make([]sectionRow, 0, len(snapshot.Disks))
		for _, disk := range snapshot.Disks {
			rows = append(rows, row(fmt.Sprintf("%s %s", disk.Device, disk.Mount), fmt.Sprintf("%s / %s", formatBytes(disk.UsedBytes), formatBytes(disk.TotalBytes))))
		}
		children = append(children, buildDetailSection("Disk", rows))
	}

	if config.Popover.Sections.Temps && len(snapshot.Temperatures) > 0 {
		rows := make([]sectionRow, 0, len(snapshot.Temperatures))
		for _, reading := range snapshot.Temperatures {
			rows = append(rows, row(reading.Label, fmt.Sprintf("%dC", rounded(reading.Celsius))))
		}
		children = append(children, buildDetailSection("Temperatures", rows))
	}

	if config.Popover.Sections.Uptime {
		children = append(children, buildDetailSection(
			"Uptime",
			appendRows(
				row("Uptime", formatUptime(snapshot.Uptime)),
				optionalUpdatedRow(snapshot.UpdatedAt),
			),
		))
	}

	tree := sdk.BoxVertical(children, 10)
	return &tree
}

func buildHeroNode(snapshot Snapshot) sdk.TreeNode {
	hero := sdk.NewHero(
		"System Stats",
		fmt.Sprintf(
			"CPU %d%% · RAM %d%% · Swap %d%%",
			rounded(snapshot.CPUPercent),
			rounded(snapshot.Memory.UsedPercent),
			rounded(snapshot.Swap.UsedPercent),
		),
	)
	heroData := hero.Data.(sdk.Hero)
	heroData.Icon = iconName("computer-symbolic")
	hero.Data = heroData
	return hero
}

type sectionRow struct {
	Key   string
	Value string
}

func buildDetailSection(title string, rows []sectionRow) sdk.TreeNode {
	return buildSectionWithFallback(title, rows, "No data available", "")
}

func buildSectionWithFallback(title string, rows []sectionRow, emptyTitle string, emptySubtitle string) sdk.TreeNode {
	children := []sdk.TreeNode{}
	if len(rows) > 0 {
		children = append(children, sdk.NewPropertyList(buildProperties(rows)))
	} else {
		empty := sdk.NewEmptyState(emptyTitle)
		emptyState := empty.Data.(sdk.EmptyState)
		emptyState.Subtitle = emptySubtitle
		empty.Data = emptyState
		children = append(children, empty)
	}
	return sdk.NewSection(title, children)
}

func buildProperties(rows []sectionRow) sdk.Properties {
	properties := sdk.Properties{}
	for _, row := range rows {
		properties[row.Key] = row.Value
	}
	return properties
}

func appendRows(rows ...sectionRow) []sectionRow {
	out := make([]sectionRow, 0, len(rows))
	for _, row := range rows {
		if row.Key == "" || row.Value == "" {
			continue
		}
		out = append(out, row)
	}
	return out
}

func row(key, value string) sectionRow {
	return sectionRow{Key: key, Value: value}
}

func optionalRow(key, value string) sectionRow {
	if value == "" {
		return sectionRow{}
	}
	return row(key, value)
}

func conditionalRow(enabled bool, key, value string) sectionRow {
	if !enabled {
		return sectionRow{}
	}
	return row(key, value)
}

func optionalInterfaceRow(stats NetworkStats) sectionRow {
	if stats.Interface == "" {
		return sectionRow{}
	}
	return row("Active interface", formatInterface(stats))
}

func optionalUpdatedRow(updatedAt time.Time) sectionRow {
	if updatedAt.IsZero() {
		return sectionRow{}
	}
	return row("Updated", updatedAt.Local().Format("15:04:05"))
}

func optionalWarningLevels(config Config) sectionRow {
	parts := []string{}
	if config.Panel.Items.RAM.WarnAt > 0 {
		parts = append(parts, fmt.Sprintf("RAM %d%%", config.Panel.Items.RAM.WarnAt))
	}
	if config.Panel.Items.Swap.WarnAt > 0 {
		parts = append(parts, fmt.Sprintf("Swap %d%%", config.Panel.Items.Swap.WarnAt))
	}
	if len(parts) == 0 {
		return sectionRow{}
	}
	return row("Warning levels", strings.Join(parts, " · "))
}

func thresholdExceeded(percent float64, threshold int) bool {
	return threshold > 0 && percent >= float64(threshold)
}

func iconName(value string) *sdk.Icon {
	if value == "" {
		return nil
	}
	return sdk.IconName(value)
}

func rounded(value float64) int {
	return int(math.Round(value))
}

func formatBytes(value uint64) string {
	if value == 0 {
		return "0 B"
	}
	units := []string{"B", "KiB", "MiB", "GiB", "TiB"}
	size := float64(value)
	unitIdx := 0
	for size >= 1024 && unitIdx < len(units)-1 {
		size /= 1024
		unitIdx++
	}
	if size >= 10 || unitIdx == 0 {
		return fmt.Sprintf("%.0f %s", size, units[unitIdx])
	}
	return fmt.Sprintf("%.1f %s", size, units[unitIdx])
}

func formatUptime(value time.Duration) string {
	totalMinutes := int(value.Minutes())
	days := totalMinutes / (24 * 60)
	hours := (totalMinutes % (24 * 60)) / 60
	minutes := totalMinutes % 60

	if days > 0 {
		return fmt.Sprintf("%dd %02dh %02dm", days, hours, minutes)
	}
	return fmt.Sprintf("%02dh %02dm", hours, minutes)
}

func formatInterface(stats NetworkStats) string {
	if stats.Interface == "" {
		return "Unavailable"
	}
	if stats.State == "" {
		return stats.Interface
	}
	return fmt.Sprintf("%s (%s)", stats.Interface, stats.State)
}

func formatThroughput(stats NetworkStats) string {
	if stats.RXBytesPerSec <= 0 && stats.TXBytesPerSec <= 0 {
		return ""
	}
	return fmt.Sprintf("↓ %s/s  ↑ %s/s", formatBytes(uint64(stats.RXBytesPerSec)), formatBytes(uint64(stats.TXBytesPerSec)))
}

func formatGPU(gpu GPUStats) string {
	parts := []string{fmt.Sprintf("%d%%", rounded(gpu.UtilPercent))}
	if gpu.MemoryTotalBytes > 0 {
		parts = append(parts, fmt.Sprintf("%s / %s", formatBytes(gpu.MemoryUsedBytes), formatBytes(gpu.MemoryTotalBytes)))
	}
	if gpu.TempCelsius > 0 {
		parts = append(parts, fmt.Sprintf("%dC", rounded(gpu.TempCelsius)))
	}
	return strings.Join(parts, " · ")
}
