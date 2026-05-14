package main

import (
	"bufio"
	"context"
	"encoding/csv"
	"errors"
	"io"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"
	"time"
)

const gib = 1024 * 1024 * 1024

type Snapshot struct {
	CPUPercent   float64
	Memory       MemoryStats
	Swap         SwapStats
	Load         LoadStats
	Network      NetworkStats
	GPUs         []GPUStats
	Disks        []DiskStats
	Temperatures []TemperatureReading
	Uptime       time.Duration
	UpdatedAt    time.Time
}

type MemoryStats struct {
	UsedBytes   uint64
	TotalBytes  uint64
	UsedPercent float64
}

type SwapStats struct {
	UsedBytes   uint64
	TotalBytes  uint64
	UsedPercent float64
}

type LoadStats struct {
	One     float64
	Five    float64
	Fifteen float64
}

type NetworkStats struct {
	Interface     string
	State         string
	IPv4          string
	RXBytesPerSec float64
	TXBytesPerSec float64
}

type GPUStats struct {
	Name             string
	Vendor           string
	UtilPercent      float64
	MemoryUsedBytes  uint64
	MemoryTotalBytes uint64
	TempCelsius      float64
	Powered          bool
}

func (g GPUStats) Label() string {
	if g.Name == "" {
		return g.Vendor
	}
	return g.Name
}

type DiskStats struct {
	Device     string
	Mount      string
	UsedBytes  uint64
	TotalBytes uint64
}

type TemperatureReading struct {
	Label   string
	Celsius float64
}

type Collector interface {
	Collect(context.Context) (Snapshot, error)
}

type LinuxCollector struct {
	procRoot string
	sysRoot  string

	execCommand func(context.Context, string, ...string) ([]byte, error)
	now         func() time.Time

	prevCPU     cpuCounters
	havePrevCPU bool
	prevNet     map[string]netCounters
	prevNetAt   time.Time
}

func newLinuxCollector() *LinuxCollector {
	return &LinuxCollector{
		procRoot: "/proc",
		sysRoot:  "/sys",
		execCommand: func(ctx context.Context, name string, args ...string) ([]byte, error) {
			cmd := exec.CommandContext(ctx, name, args...)
			return cmd.Output()
		},
		now:     time.Now,
		prevNet: make(map[string]netCounters),
	}
}

func (c *LinuxCollector) Collect(ctx context.Context) (Snapshot, error) {
	now := c.now()
	snapshot := Snapshot{UpdatedAt: now}

	if cpuPercent, load, err := c.collectCPU(ctx); err == nil {
		snapshot.CPUPercent = cpuPercent
		snapshot.Load = load
	}

	if memory, swap, err := c.collectMemory(); err == nil {
		snapshot.Memory = memory
		snapshot.Swap = swap
	}

	if network, err := c.collectNetwork(now); err == nil {
		snapshot.Network = network
	}

	nvidia, _ := c.collectNvidiaGPU(ctx)
	amd, _ := c.collectAMDGPU()
	snapshot.GPUs = append(snapshot.GPUs, nvidia...)
	snapshot.GPUs = append(snapshot.GPUs, amd...)

	if disks, err := c.collectDisks(); err == nil {
		snapshot.Disks = disks
	}

	temps := make([]TemperatureReading, 0, 1+len(snapshot.GPUs))
	if cpuTemp, ok := c.collectPrimaryCPUTemp(); ok {
		temps = append(temps, cpuTemp)
	}
	for _, gpu := range snapshot.GPUs {
		if gpu.TempCelsius > 0 {
			temps = append(temps, TemperatureReading{Label: gpu.Label(), Celsius: gpu.TempCelsius})
		}
	}
	snapshot.Temperatures = temps

	if uptime, err := c.collectUptime(); err == nil {
		snapshot.Uptime = uptime
	}

	return snapshot, nil
}

type cpuCounters struct {
	idle  uint64
	total uint64
}

type netCounters struct {
	rx uint64
	tx uint64
}

func (c *LinuxCollector) collectCPU(ctx context.Context) (float64, LoadStats, error) {
	load, err := parseLoadavgFile(filepath.Join(c.procRoot, "loadavg"))
	if err != nil {
		return 0, LoadStats{}, err
	}

	current, err := readCPUCounters(filepath.Join(c.procRoot, "stat"))
	if err != nil {
		return 0, LoadStats{}, err
	}

	if !c.havePrevCPU {
		select {
		case <-ctx.Done():
			return 0, load, ctx.Err()
		case <-time.After(120 * time.Millisecond):
		}
		next, err := readCPUCounters(filepath.Join(c.procRoot, "stat"))
		if err != nil {
			return 0, load, err
		}
		c.prevCPU = next
		c.havePrevCPU = true
		return calculateCPUPercent(current, next), load, nil
	}

	percent := calculateCPUPercent(c.prevCPU, current)
	c.prevCPU = current
	return percent, load, nil
}

func readCPUCounters(path string) (cpuCounters, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return cpuCounters{}, err
	}
	lines := strings.Split(string(data), "\n")
	for _, line := range lines {
		if !strings.HasPrefix(line, "cpu ") {
			continue
		}
		fields := strings.Fields(line)
		if len(fields) < 8 {
			return cpuCounters{}, errors.New("unexpected /proc/stat format")
		}
		values := make([]uint64, 0, len(fields)-1)
		for _, field := range fields[1:] {
			value, err := strconv.ParseUint(field, 10, 64)
			if err != nil {
				return cpuCounters{}, err
			}
			values = append(values, value)
		}
		var total uint64
		for _, value := range values {
			total += value
		}
		idle := values[3]
		if len(values) > 4 {
			idle += values[4]
		}
		return cpuCounters{idle: idle, total: total}, nil
	}
	return cpuCounters{}, errors.New("cpu counters missing")
}

func calculateCPUPercent(prev, next cpuCounters) float64 {
	totalDelta := float64(next.total - prev.total)
	idleDelta := float64(next.idle - prev.idle)
	if totalDelta <= 0 {
		return 0
	}
	return (1 - idleDelta/totalDelta) * 100
}

func parseLoadavgFile(path string) (LoadStats, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return LoadStats{}, err
	}
	fields := strings.Fields(string(data))
	if len(fields) < 3 {
		return LoadStats{}, errors.New("unexpected /proc/loadavg format")
	}
	one, _ := strconv.ParseFloat(fields[0], 64)
	five, _ := strconv.ParseFloat(fields[1], 64)
	fifteen, _ := strconv.ParseFloat(fields[2], 64)
	return LoadStats{One: one, Five: five, Fifteen: fifteen}, nil
}

func (c *LinuxCollector) collectMemory() (MemoryStats, SwapStats, error) {
	file, err := os.Open(filepath.Join(c.procRoot, "meminfo"))
	if err != nil {
		return MemoryStats{}, SwapStats{}, err
	}
	defer file.Close()
	return parseMeminfo(file)
}

func parseMeminfo(reader io.Reader) (MemoryStats, SwapStats, error) {
	values := make(map[string]uint64)
	scanner := bufio.NewScanner(reader)
	for scanner.Scan() {
		fields := strings.Fields(scanner.Text())
		if len(fields) < 2 {
			continue
		}
		key := strings.TrimSuffix(fields[0], ":")
		value, err := strconv.ParseUint(fields[1], 10, 64)
		if err != nil {
			return MemoryStats{}, SwapStats{}, err
		}
		values[key] = value * 1024
	}
	if err := scanner.Err(); err != nil {
		return MemoryStats{}, SwapStats{}, err
	}

	memTotal := values["MemTotal"]
	memAvailable := values["MemAvailable"]
	if memAvailable == 0 {
		memAvailable = values["MemFree"] + values["Buffers"] + values["Cached"]
	}
	memUsed := uint64(0)
	if memTotal > memAvailable {
		memUsed = memTotal - memAvailable
	}

	swapTotal := values["SwapTotal"]
	swapFree := values["SwapFree"]
	swapUsed := uint64(0)
	if swapTotal > swapFree {
		swapUsed = swapTotal - swapFree
	}

	return MemoryStats{
			UsedBytes:   memUsed,
			TotalBytes:  memTotal,
			UsedPercent: percent(memUsed, memTotal),
		}, SwapStats{
			UsedBytes:   swapUsed,
			TotalBytes:  swapTotal,
			UsedPercent: percent(swapUsed, swapTotal),
		}, nil
}

func percent(used, total uint64) float64 {
	if total == 0 {
		return 0
	}
	return (float64(used) / float64(total)) * 100
}

func (c *LinuxCollector) collectNetwork(now time.Time) (NetworkStats, error) {
	iface, err := c.resolvePrimaryInterface()
	if err != nil {
		return NetworkStats{}, err
	}

	counters, err := readNetDev(filepath.Join(c.procRoot, "net", "dev"))
	if err != nil {
		return NetworkStats{}, err
	}
	current, ok := counters[iface]
	if !ok {
		return NetworkStats{}, errors.New("network interface counters missing")
	}

	stats := NetworkStats{
		Interface: iface,
		State:     strings.TrimSpace(readTrimmed(filepath.Join(c.sysRoot, "class", "net", iface, "operstate"))),
		IPv4:      interfaceIPv4(iface),
	}

	if prev, ok := c.prevNet[iface]; ok && !c.prevNetAt.IsZero() {
		elapsed := now.Sub(c.prevNetAt).Seconds()
		if elapsed > 0 {
			stats.RXBytesPerSec = float64(current.rx-prev.rx) / elapsed
			stats.TXBytesPerSec = float64(current.tx-prev.tx) / elapsed
		}
	}

	c.prevNet[iface] = current
	c.prevNetAt = now
	return stats, nil
}

func (c *LinuxCollector) resolvePrimaryInterface() (string, error) {
	routePath := filepath.Join(c.procRoot, "net", "route")
	file, err := os.Open(routePath)
	if err == nil {
		defer file.Close()
		scanner := bufio.NewScanner(file)
		for scanner.Scan() {
			fields := strings.Fields(scanner.Text())
			if len(fields) >= 2 && fields[1] == "00000000" && fields[0] != "lo" {
				return fields[0], nil
			}
		}
	}

	entries, err := os.ReadDir(filepath.Join(c.sysRoot, "class", "net"))
	if err != nil {
		return "", err
	}
	for _, entry := range entries {
		name := entry.Name()
		if name == "lo" {
			continue
		}
		state := strings.TrimSpace(readTrimmed(filepath.Join(c.sysRoot, "class", "net", name, "operstate")))
		if state == "up" || state == "unknown" {
			return name, nil
		}
	}
	return "", errors.New("no active network interface")
}

func readNetDev(path string) (map[string]netCounters, error) {
	file, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer file.Close()

	out := make(map[string]netCounters)
	scanner := bufio.NewScanner(file)
	for scanner.Scan() {
		line := scanner.Text()
		if !strings.Contains(line, ":") {
			continue
		}
		name, rest, _ := strings.Cut(line, ":")
		fields := strings.Fields(rest)
		if len(fields) < 16 {
			continue
		}
		rx, _ := strconv.ParseUint(fields[0], 10, 64)
		tx, _ := strconv.ParseUint(fields[8], 10, 64)
		out[strings.TrimSpace(name)] = netCounters{rx: rx, tx: tx}
	}
	return out, scanner.Err()
}

func interfaceIPv4(name string) string {
	iface, err := net.InterfaceByName(name)
	if err != nil {
		return ""
	}
	addrs, err := iface.Addrs()
	if err != nil {
		return ""
	}
	for _, addr := range addrs {
		if ipNet, ok := addr.(*net.IPNet); ok && ipNet.IP.To4() != nil {
			return ipNet.IP.String()
		}
	}
	return ""
}

func (c *LinuxCollector) collectNvidiaGPU(ctx context.Context) ([]GPUStats, error) {
	if _, err := exec.LookPath("nvidia-smi"); err != nil {
		return nil, err
	}
	output, err := c.execCommand(
		ctx,
		"nvidia-smi",
		"--query-gpu=index,name,utilization.gpu,memory.used,memory.total,temperature.gpu,persistence_mode",
		"--format=csv,noheader,nounits",
	)
	if err != nil {
		return nil, err
	}
	return parseNvidiaSMI(strings.NewReader(string(output)))
}

func parseNvidiaSMI(reader io.Reader) ([]GPUStats, error) {
	csvReader := csv.NewReader(reader)
	csvReader.TrimLeadingSpace = true

	var out []GPUStats
	for {
		record, err := csvReader.Read()
		if errors.Is(err, io.EOF) {
			return out, nil
		}
		if err != nil {
			return nil, err
		}
		if len(record) < 7 {
			return nil, errors.New("unexpected nvidia-smi column count")
		}
		util, _ := strconv.ParseFloat(strings.TrimSpace(record[2]), 64)
		memUsedMiB, _ := strconv.ParseUint(strings.TrimSpace(record[3]), 10, 64)
		memTotalMiB, _ := strconv.ParseUint(strings.TrimSpace(record[4]), 10, 64)
		temp, _ := strconv.ParseFloat(strings.TrimSpace(record[5]), 64)
		powered := !strings.EqualFold(strings.TrimSpace(record[6]), "Disabled")

		out = append(out, GPUStats{
			Name:             strings.TrimSpace(record[1]),
			Vendor:           "NVIDIA",
			UtilPercent:      util,
			MemoryUsedBytes:  memUsedMiB * 1024 * 1024,
			MemoryTotalBytes: memTotalMiB * 1024 * 1024,
			TempCelsius:      temp,
			Powered:          powered,
		})
	}
}

func (c *LinuxCollector) collectAMDGPU() ([]GPUStats, error) {
	pattern := filepath.Join(c.sysRoot, "class", "drm", "card*")
	cards, err := filepath.Glob(pattern)
	if err != nil {
		return nil, err
	}

	var out []GPUStats
	for _, card := range cards {
		deviceDir := filepath.Join(card, "device")
		vendor := strings.TrimSpace(readTrimmed(filepath.Join(deviceDir, "vendor")))
		if vendor != "0x1002" {
			continue
		}

		runtimeStatus := strings.TrimSpace(readTrimmed(filepath.Join(deviceDir, "power", "runtime_status")))
		if runtimeStatus == "suspended" {
			continue
		}

		util, _ := strconv.ParseFloat(strings.TrimSpace(readTrimmed(filepath.Join(deviceDir, "gpu_busy_percent"))), 64)
		vramUsed, _ := strconv.ParseUint(strings.TrimSpace(readTrimmed(filepath.Join(deviceDir, "mem_info_vram_used"))), 10, 64)
		vramTotal, _ := strconv.ParseUint(strings.TrimSpace(readTrimmed(filepath.Join(deviceDir, "mem_info_vram_total"))), 10, 64)
		temp := readHWMonTemp(deviceDir)
		name := filepath.Base(card)
		if productName := strings.TrimSpace(readTrimmed(filepath.Join(deviceDir, "product_name"))); productName != "" {
			name = productName
		}

		out = append(out, GPUStats{
			Name:             name,
			Vendor:           "AMD",
			UtilPercent:      util,
			MemoryUsedBytes:  vramUsed,
			MemoryTotalBytes: vramTotal,
			TempCelsius:      temp,
			Powered:          true,
		})
	}
	return out, nil
}

func (c *LinuxCollector) collectDisks() ([]DiskStats, error) {
	file, err := os.Open(filepath.Join(c.procRoot, "self", "mounts"))
	if err != nil {
		return nil, err
	}
	defer file.Close()

	var out []DiskStats
	seen := make(map[string]struct{})
	scanner := bufio.NewScanner(file)
	for scanner.Scan() {
		fields := strings.Fields(scanner.Text())
		if len(fields) < 2 {
			continue
		}
		source := fields[0]
		mountPoint := fields[1]
		if !strings.HasPrefix(source, "/dev/") {
			continue
		}
		displayDevice := filepath.Base(source)
		resolvedSource, err := filepath.EvalSymlinks(source)
		if err != nil {
			resolvedSource = source
		}
		device := filepath.Base(resolvedSource)
		if strings.HasPrefix(device, "loop") || strings.HasPrefix(device, "zram") {
			continue
		}
		sysPath := filepath.Join(c.sysRoot, "class", "block", device)
		info, err := os.Stat(sysPath)
		if err != nil || !info.IsDir() {
			continue
		}
		if strings.Contains(readLink(sysPath), "/virtual/") {
			continue
		}
		key := source + "@" + mountPoint
		if _, ok := seen[key]; ok {
			continue
		}
		var stat syscall.Statfs_t
		if err := syscall.Statfs(mountPoint, &stat); err != nil {
			continue
		}
		total := stat.Blocks * uint64(stat.Bsize)
		used := (stat.Blocks - stat.Bavail) * uint64(stat.Bsize)
		out = append(out, DiskStats{
			Device:     displayDevice,
			Mount:      mountPoint,
			UsedBytes:  used,
			TotalBytes: total,
		})
		seen[key] = struct{}{}
	}
	return out, scanner.Err()
}

func (c *LinuxCollector) collectPrimaryCPUTemp() (TemperatureReading, bool) {
	thermalZones, _ := filepath.Glob(filepath.Join(c.sysRoot, "class", "thermal", "thermal_zone*"))
	preferred := []string{"x86_pkg_temp", "cpu-thermal", "k10temp", "soc_thermal"}
	for _, want := range preferred {
		for _, zone := range thermalZones {
			zoneType := strings.TrimSpace(readTrimmed(filepath.Join(zone, "type")))
			if zoneType != want {
				continue
			}
			tempMilli, err := strconv.ParseFloat(strings.TrimSpace(readTrimmed(filepath.Join(zone, "temp"))), 64)
			if err != nil || tempMilli <= 0 {
				continue
			}
			return TemperatureReading{Label: "CPU", Celsius: tempMilli / 1000}, true
		}
	}

	hwmons, _ := filepath.Glob(filepath.Join(c.sysRoot, "class", "hwmon", "hwmon*"))
	for _, want := range []string{"k10temp", "coretemp", "zenpower", "acpitz"} {
		for _, hwmon := range hwmons {
			name := strings.TrimSpace(readTrimmed(filepath.Join(hwmon, "name")))
			if name != want {
				continue
			}
			for _, sensor := range []string{"temp1_input", "temp2_input"} {
				value, err := strconv.ParseFloat(strings.TrimSpace(readTrimmed(filepath.Join(hwmon, sensor))), 64)
				if err == nil && value > 0 {
					return TemperatureReading{Label: "CPU", Celsius: value / 1000}, true
				}
			}
		}
	}
	return TemperatureReading{}, false
}

func (c *LinuxCollector) collectUptime() (time.Duration, error) {
	data, err := os.ReadFile(filepath.Join(c.procRoot, "uptime"))
	if err != nil {
		return 0, err
	}
	fields := strings.Fields(string(data))
	if len(fields) == 0 {
		return 0, errors.New("unexpected /proc/uptime format")
	}
	seconds, err := strconv.ParseFloat(fields[0], 64)
	if err != nil {
		return 0, err
	}
	return time.Duration(seconds * float64(time.Second)), nil
}

func readTrimmed(path string) string {
	data, err := os.ReadFile(path)
	if err != nil {
		return ""
	}
	return string(data)
}

func readLink(path string) string {
	target, err := os.Readlink(path)
	if err != nil {
		return ""
	}
	return target
}

func readHWMonTemp(deviceDir string) float64 {
	hwmons, _ := filepath.Glob(filepath.Join(deviceDir, "hwmon", "hwmon*"))
	for _, hwmon := range hwmons {
		value, err := strconv.ParseFloat(strings.TrimSpace(readTrimmed(filepath.Join(hwmon, "temp1_input"))), 64)
		if err == nil && value > 0 {
			return value / 1000
		}
	}
	return 0
}
