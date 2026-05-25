//! Builds the sysmonitor popover tree from the latest sample. Every
//! section is conditionally emitted — unavailable hardware (no swap,
//! no GPU, no temps) doesn't produce an empty tile. This is the
//! "popover should not display unavailable values" requirement.


use glimpse_sdk::{
    BoxedList, Column, EmptyState, ExpanderTile, Hero, KeyValueGrid, KeyValueRow, MsgMapper,
    PopoverShell, PopoverSize, Row, Tile, TreeNode, tree,
};

use crate::Msg;
use crate::samplers::{
    DiskSample, GpuSample, MemSample, NetSample, ProcessSample, Sample, TempSample,
};

pub fn build(sample: &Sample, current_uid: u32) -> TreeNode<Msg> {
    let mut sections: Vec<TreeNode<Msg>> = Vec::new();

    sections.push(cpu_hero(sample));

    if sample.mem.total_bytes > 0 {
        sections.push(memory_tile("Memory", "drive-harddisk-system-symbolic", &sample.mem));
    }
    // Hide the swap row entirely when no swap is configured — empty
    // SwapTotal would otherwise show as "0.00 GiB / 0.00 GiB" which is
    // misleading rather than informative.
    if sample.swap.total_bytes > 0 {
        sections.push(memory_tile("Swap", "drive-harddisk-symbolic", &sample.swap));
    }

    if !sample.disks.is_empty() {
        sections.push(disks_section(&sample.disks));
    }
    if !sample.nets.is_empty() {
        sections.push(network_section(&sample.nets));
    }
    if !sample.temps.is_empty() {
        sections.push(temperature_grid(&sample.temps));
    }
    if let Some(gpu) = &sample.amdgpu {
        sections.push(gpu_hero("AMD GPU", "video-display-symbolic", gpu));
    }
    if let Some(gpu) = &sample.nvidia {
        sections.push(gpu_hero("NVIDIA GPU", "video-display-symbolic", gpu));
    }

    if !sample.procs.top_cpu.is_empty() {
        sections.push(top_processes_expander(
            "Top CPU",
            "utilities-system-monitor-symbolic",
            &sample.procs.top_cpu,
            current_uid,
            ProcessSortBy::Cpu,
        ));
    }
    if !sample.procs.top_rss.is_empty() {
        sections.push(top_processes_expander(
            "Top Memory",
            "drive-harddisk-system-symbolic",
            &sample.procs.top_rss,
            current_uid,
            ProcessSortBy::Rss,
        ));
    }

    if sections.is_empty() {
        // First tick before the sampler has produced anything.
        let empty = EmptyState::new("Collecting metrics…");
        let mut shell = PopoverShell::new(tree![empty]);
        shell.size = PopoverSize::Medium;
        return shell.into();
    }

    let mut shell = PopoverShell::new(sections);
    shell.size = PopoverSize::Medium;
    shell.into()
}

fn cpu_hero(sample: &Sample) -> TreeNode<Msg> {
    let cpu = &sample.cpu;
    let mut parts = Vec::new();
    parts.push(format!("{:.0}% util", cpu.util * 100.0));
    if cpu.freq_mhz > 0.0 {
        parts.push(format!("{:.2} GHz", cpu.freq_mhz / 1000.0));
    }
    if let Some(t) = cpu.temp_c {
        parts.push(format!("{t:.0}°C"));
    }
    if cpu.load_1 > 0.0 {
        parts.push(format!(
            "load {:.2} / {:.2} / {:.2}",
            cpu.load_1, cpu.load_5, cpu.load_15
        ));
    }
    let mut hero = Hero::new("CPU", parts.join(" • "));
    hero.icon = Some("cpu-symbolic".into());
    hero.into()
}

fn memory_tile(name: &str, icon: &str, mem: &MemSample) -> TreeNode<Msg> {
    let to_gib = |b: u64| (b as f64) / (1024.0 * 1024.0 * 1024.0);
    let mut tile = Tile::new(name);
    tile.activatable = false;
    tile.left_icon = Some(icon.into());
    tile.secondary = Some(format!(
        "{:.1} / {:.1} GiB ({:.0}%)",
        to_gib(mem.used_bytes),
        to_gib(mem.total_bytes),
        mem.util * 100.0,
    ));
    tile.into()
}

fn disks_section(disks: &std::collections::HashMap<String, DiskSample>) -> TreeNode<Msg> {
    // Stable order keyed by mountpoint so identical inputs produce the
    // same rendered tree — important for the shell's diff-based renderer.
    let mut entries: Vec<(&String, &DiskSample)> = disks.iter().collect();
    // Cloning the (short) mountpoint string keeps sort_by_key's `K: Ord`
    // bound happy without an explicit lifetime dance — the typical user
    // configures one or two disks, so the cost is irrelevant.
    entries.sort_by_key(|(k, _)| (*k).clone());
    let tiles: Vec<TreeNode<Msg>> = entries
        .into_iter()
        .map(|(mp, disk)| {
            let to_gib = |b: u64| (b as f64) / (1024.0 * 1024.0 * 1024.0);
            let mut tile = Tile::new(mp.clone());
            tile.activatable = false;
            tile.left_icon = Some("drive-harddisk-symbolic".into());
            tile.secondary = Some(format!(
                "{:.1} / {:.1} GiB ({:.0}%)",
                to_gib(disk.used_bytes),
                to_gib(disk.total_bytes),
                disk.util * 100.0,
            ));
            tile.into()
        })
        .collect();
    BoxedList::new(tiles).into()
}

fn network_section(nets: &std::collections::HashMap<String, NetSample>) -> TreeNode<Msg> {
    let mut entries: Vec<(&String, &NetSample)> = nets.iter().collect();
    entries.sort_by_key(|(k, _)| (*k).clone());
    let tiles: Vec<TreeNode<Msg>> = entries
        .into_iter()
        .map(|(iface, net)| {
            let mut tile = Tile::new(iface.clone());
            tile.activatable = false;
            tile.left_icon = Some("network-wired-symbolic".into());
            tile.secondary = Some(format!(
                "↑ {:.1} KiB/s · ↓ {:.1} KiB/s",
                net.tx_bytes_per_sec / 1024.0,
                net.rx_bytes_per_sec / 1024.0,
            ));
            tile.into()
        })
        .collect();
    BoxedList::new(tiles).into()
}

fn temperature_grid(temps: &std::collections::HashMap<String, TempSample>) -> TreeNode<Msg> {
    let mut entries: Vec<(&String, &TempSample)> = temps.iter().collect();
    entries.sort_by_key(|(k, _)| (*k).clone());
    let rows: Vec<KeyValueRow> = entries
        .into_iter()
        .map(|(spec, t)| KeyValueRow {
            key: t.sensor_label.clone().unwrap_or_else(|| spec.clone()),
            value: format!("{:.0} °C", t.temp_c),
        })
        .collect();
    KeyValueGrid::new(rows).into()
}

fn gpu_hero(title: &str, icon: &str, gpu: &GpuSample) -> TreeNode<Msg> {
    let mut parts = Vec::new();
    if let Some(u) = gpu.util {
        parts.push(format!("{:.0}% util", u * 100.0));
    }
    if let Some(t) = gpu.temp_c {
        parts.push(format!("{t:.0}°C"));
    }
    if let (Some(used), Some(total)) = (gpu.mem_used_bytes, gpu.mem_total_bytes) {
        let to_gib = |b: u64| (b as f64) / (1024.0 * 1024.0 * 1024.0);
        parts.push(format!("{:.1} / {:.1} GiB VRAM", to_gib(used), to_gib(total)));
    }
    if let Some(f) = gpu.freq_mhz {
        parts.push(format!("{f:.0} MHz"));
    }
    if let Some(p) = gpu.power_w {
        parts.push(format!("{p:.0} W"));
    }
    let subtitle = if parts.is_empty() {
        gpu.name.clone().unwrap_or_else(|| "available".into())
    } else {
        parts.join(" • ")
    };
    let label = match (&gpu.name, title) {
        (Some(n), _) => n.clone(),
        (None, t) => t.to_string(),
    };
    let mut hero = Hero::new(label, subtitle);
    hero.icon = Some(icon.into());
    hero.into()
}

#[derive(Copy, Clone)]
enum ProcessSortBy {
    Cpu,
    Rss,
}

fn top_processes_expander(
    title: &str,
    icon: &str,
    procs: &[ProcessSample],
    current_uid: u32,
    sort_by: ProcessSortBy,
) -> TreeNode<Msg> {
    let rows: Vec<TreeNode<Msg>> = procs
        .iter()
        .map(|p| process_row(p, current_uid, sort_by))
        .collect();
    let list: TreeNode<Msg> = BoxedList::new(rows).into();
    let mut expander = ExpanderTile::new(title);
    expander.left_icon = Some(icon.into());
    expander.child = Some(Box::new(list));
    expander.into()
}

fn process_row(p: &ProcessSample, current_uid: u32, sort_by: ProcessSortBy) -> TreeNode<Msg> {
    let comm = if p.comm.is_empty() {
        format!("pid {}", p.pid)
    } else {
        p.comm.clone()
    };
    let mib = |b: u64| (b as f64) / (1024.0 * 1024.0);
    let stats = match sort_by {
        // Two views, same row shape — primary metric on the left, the
        // other metric trailing, so users sorted by CPU still see the
        // process's RAM footprint and vice versa.
        ProcessSortBy::Cpu => format!(
            "PID {} · {:.0}% CPU · {:.0} MiB",
            p.pid,
            p.cpu_pct * 100.0,
            mib(p.rss_bytes),
        ),
        ProcessSortBy::Rss => format!(
            "PID {} · {:.0} MiB · {:.0}% CPU",
            p.pid,
            mib(p.rss_bytes),
            p.cpu_pct * 100.0,
        ),
    };
    let mut info = Tile::new(comm);
    info.activatable = false;
    info.secondary = Some(stats);
    info.left_icon = Some("application-x-executable-symbolic".into());

    if p.uid != current_uid {
        // Foreign-owned process — no actionable buttons; just the
        // informational row. kill(2) would EPERM us anyway.
        return info.into();
    }

    let pid = p.pid;
    let mut term = Tile::new("Terminate");
    term.activatable = true;
    term.left_icon = Some("media-playback-stop-symbolic".into());
    term.secondary = Some("SIGTERM".into());
    term.on_click = Some(MsgMapper::new(move |()| Msg::TerminateProc(pid)));

    let mut kill = Tile::new("Kill");
    kill.activatable = true;
    kill.left_icon = Some("process-stop-symbolic".into());
    kill.secondary = Some("SIGKILL".into());
    kill.on_click = Some(MsgMapper::new(move |()| Msg::KillProc(pid)));

    let actions: TreeNode<Msg> = Row::new(tree![term, kill]).into();
    Column::new(tree![info, actions]).into()
}
