use std::collections::HashMap;
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::os::fd::AsFd;

use glimpse_services::{Gamma, NEUTRAL_KELVIN as NEUTRAL};
use rustix::fs::{MemfdFlags, memfd_create};
use wayland_client::protocol::{wl_output, wl_registry};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, delegate_noop};
use wayland_protocols_wlr::gamma_control::v1::client::{
    zwlr_gamma_control_manager_v1::ZwlrGammaControlManagerV1,
    zwlr_gamma_control_v1::{self, ZwlrGammaControlV1},
};

pub struct WaylandGamma {
    connection: Connection,
    queue: EventQueue<Outputs>,
    registry: wl_registry::WlRegistry,
    outputs: Outputs,
}

#[derive(Debug)]
pub enum Unavailable {
    Unsupported,
    Unreachable(String),
}

impl std::fmt::Display for Unavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported => {
                f.write_str("the compositor does not offer zwlr_gamma_control_manager_v1")
            }
            Self::Unreachable(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for Unavailable {}

impl WaylandGamma {
    pub fn connect() -> Result<Self, Unavailable> {
        let connection =
            Connection::connect_to_env().map_err(|error| Unavailable::Unreachable(say(error)))?;
        let mut queue = connection.new_event_queue();
        let registry = connection.display().get_registry(&queue.handle(), ());

        let mut outputs = Outputs::default();
        queue
            .roundtrip(&mut outputs)
            .map_err(|error| Unavailable::Unreachable(say(error)))?;
        if outputs.manager.is_none() {
            return Err(Unavailable::Unsupported);
        }

        Ok(Self {
            connection,
            queue,
            registry,
            outputs,
        })
    }

    fn revive(&mut self) -> Result<&mut Self, String> {
        if let Err(error) = self.connection.flush() {
            tracing::warn!(%error, "the compositor connection is dead, reconnecting");
            *self = Self::connect().map_err(say)?;
        }
        Ok(self)
    }

    fn set(&mut self, kelvin: u32) -> Result<(), String> {
        self.arm()?;

        // Every table is built before any is sent, so a `memfd` that cannot be created leaves the
        // outputs as they were rather than half on the new temperature and half on the old.
        let (red, green, blue) = scales(kelvin);
        let mut tables = Vec::new();
        for control in self.outputs.controls.values() {
            let Some(size) = control.size else {
                continue;
            };
            tables.push((control.control.clone(), ramp(size, red, green, blue)?));
        }
        if tables.is_empty() {
            return Err(self.refusal());
        }
        for (control, table) in &tables {
            control.set_gamma(table.as_fd());
        }

        self.connection.flush().map_err(say)?;
        self.queue.roundtrip(&mut self.outputs).map_err(say)?;
        self.outputs.discard_failed();
        match self.outputs.controls.is_empty() {
            true => Err(self.refusal()),
            false => Ok(()),
        }
    }

    fn release(&mut self) -> Result<(), String> {
        for (_, control) in self.outputs.controls.drain() {
            control.control.destroy();
        }
        self.connection.flush().map_err(say)?;
        self.queue.roundtrip(&mut self.outputs).map_err(say)?;
        Ok(())
    }

    fn arm(&mut self) -> Result<(), String> {
        let Some(manager) = self.outputs.manager.clone() else {
            return Err("the compositor does not offer gamma control".to_owned());
        };
        self.queue.roundtrip(&mut self.outputs).map_err(say)?;
        let handle = self.queue.handle();
        for (name, version) in std::mem::take(&mut self.outputs.offered) {
            let output = self.registry.bind(name, version.min(4), &handle, ());
            self.outputs.outputs.push((name, output));
        }
        let missing: Vec<(u32, wl_output::WlOutput)> = self
            .outputs
            .outputs
            .iter()
            .filter(|(name, _)| !self.outputs.controls.contains_key(name))
            .map(|(name, output)| (*name, output.clone()))
            .collect();

        if missing.is_empty() {
            return Ok(());
        }

        self.outputs.taken = false;
        for (name, output) in missing {
            let control = manager.get_gamma_control(&output, &handle, name);
            self.outputs.controls.insert(
                name,
                Control {
                    control,
                    size: None,
                    failed: false,
                },
            );
        }

        self.queue.roundtrip(&mut self.outputs).map_err(say)?;
        self.outputs.discard_failed();
        Ok(())
    }

    fn refusal(&self) -> String {
        if self.outputs.outputs.is_empty() {
            return "there are no outputs to apply a temperature to".to_owned();
        }
        match self.outputs.taken {
            true => "another gamma client holds the outputs".to_owned(),
            false => "no output accepted gamma control".to_owned(),
        }
    }
}

/// `block_in_place` because a Wayland roundtrip waits on the compositor, and a stalled one must not
/// take the runtime's worker with it. It needs the multi-threaded runtime, which every binary uses.
impl Gamma for WaylandGamma {
    fn apply(&mut self, kelvin: u32) -> Result<(), String> {
        tokio::task::block_in_place(|| self.revive()?.set(kelvin))
    }

    fn reset(&mut self) -> Result<(), String> {
        tokio::task::block_in_place(|| self.revive()?.release())
    }
}

#[derive(Default)]
struct Outputs {
    manager: Option<ZwlrGammaControlManagerV1>,
    offered: Vec<(u32, u32)>,
    outputs: Vec<(u32, wl_output::WlOutput)>,
    controls: HashMap<u32, Control>,
    taken: bool,
}

impl Outputs {
    /// wayland-rs sends no destructor on drop, so a control merely forgotten stays alive on the
    /// compositor. `arm` rebuilds one per output per tick while another client holds gamma, which
    /// is a leak measured in thousands a day rather than a handful.
    fn discard_failed(&mut self) {
        self.controls.retain(|_, control| {
            if control.failed {
                control.control.destroy();
            }
            !control.failed
        });
    }
}

struct Control {
    control: ZwlrGammaControlV1,
    size: Option<u32>,
    failed: bool,
}

impl Dispatch<wl_registry::WlRegistry, ()> for Outputs {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => match interface.as_str() {
                "zwlr_gamma_control_manager_v1" => {
                    state.manager = Some(registry.bind(name, version.min(1), handle, ()));
                }
                "wl_output" => state.offered.push((name, version)),
                _ => {}
            },
            wl_registry::Event::GlobalRemove { name } => {
                state.offered.retain(|(id, _)| *id != name);
                state.outputs.retain(|(id, _)| *id != name);
                // wayland-rs sends no destructor on drop, so a control merely forgotten stays
                // alive compositor-side — the same rule `release` and `discard_failed` follow.
                // Without this each unplug leaks one gamma control for the process's lifetime.
                if let Some(control) = state.controls.remove(&name) {
                    control.control.destroy();
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrGammaControlV1, u32> for Outputs {
    fn event(
        state: &mut Self,
        _: &ZwlrGammaControlV1,
        event: zwlr_gamma_control_v1::Event,
        name: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_gamma_control_v1::Event::GammaSize { size } => {
                if let Some(control) = state.controls.get_mut(name) {
                    control.size = Some(size);
                }
            }
            zwlr_gamma_control_v1::Event::Failed => {
                if let Some(control) = state.controls.get_mut(name) {
                    control.failed = true;
                }
                state.taken = true;
            }
            _ => {}
        }
    }
}

delegate_noop!(Outputs: ignore ZwlrGammaControlManagerV1);
delegate_noop!(Outputs: ignore wl_output::WlOutput);

/// The ramp the compositor reads, in an anonymous file that never has a path: `set_gamma` wants a
/// descriptor, and a name in `/tmp` would only be something to unlink again.
fn ramp(size: u32, red: f32, green: f32, blue: f32) -> Result<File, String> {
    let anonymous = memfd_create("glimpse-gamma", MemfdFlags::CLOEXEC).map_err(say)?;
    let mut file = File::from(anonymous);

    let mut table = Vec::with_capacity(size as usize * 3 * size_of::<u16>());
    for scale in [red, green, blue] {
        for step in 0..size {
            let progress = match size {
                0 | 1 => 1.0,
                size => step as f32 / (size - 1) as f32,
            };
            let value = ((progress * scale).clamp(0.0, 1.0) * f32::from(u16::MAX)).round() as u16;
            table.extend_from_slice(&value.to_ne_bytes());
        }
    }

    file.write_all(&table).map_err(say)?;
    file.flush().map_err(say)?;
    file.seek(SeekFrom::Start(0)).map_err(say)?;
    Ok(file)
}

/// Tanner Helland's blackbody approximation, as every night light uses, divided through by its own
/// value at `NEUTRAL`. A ramp adjusts the panel relative to a whitepoint that is already daylight,
/// so the absolute curve reads 2% low on blue at 6500K and tints every screen it is written to.
fn scales(kelvin: u32) -> (f32, f32, f32) {
    let (red, green, blue) = blackbody(kelvin);
    let (dr, dg, db) = blackbody(NEUTRAL);

    (
        (red / dr).clamp(0.0, 1.0),
        (green / dg).clamp(0.0, 1.0),
        (blue / db).clamp(0.0, 1.0),
    )
}

fn blackbody(kelvin: u32) -> (f32, f32, f32) {
    let hundreds = (kelvin as f32).clamp(1000.0, 40_000.0) / 100.0;

    let red = match hundreds <= 66.0 {
        true => 255.0,
        false => 329.698_73 * (hundreds - 60.0).powf(-0.133_204_76),
    };
    let green = match hundreds <= 66.0 {
        true => 99.470_8 * hundreds.ln() - 161.119_57,
        false => 288.122_16 * (hundreds - 60.0).powf(-0.075_514_846),
    };
    let blue = if hundreds >= 66.0 {
        255.0
    } else if hundreds <= 19.0 {
        0.0
    } else {
        138.517_73 * (hundreds - 10.0).ln() - 305.044_8
    };

    (red / 255.0, green / 255.0, blue / 255.0)
}

fn say(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 6500K is the value the night light returns to, so a curve that tinted there would tint the
    /// screen whenever nothing was meant to be applied.
    #[test]
    fn daylight_is_neutral_on_every_channel() {
        let (red, green, blue) = scales(6500);

        for channel in [red, green, blue] {
            assert!(
                (channel - 1.0).abs() < 1e-6,
                "{channel} is not neutral; a channel below one tints the screen whenever nothing \
                 is meant to be applied"
            );
        }
    }

    #[test]
    fn a_warmer_temperature_keeps_red_and_cuts_blue() {
        let (red, green, blue) = scales(3000);

        assert_eq!(red, 1.0);
        assert!(blue < green && green < red, "{red} {green} {blue}");
    }

    #[test]
    fn an_absurd_temperature_is_clamped_rather_than_refused() {
        for kelvin in [0, 1, u32::MAX] {
            let (red, green, blue) = scales(kelvin);
            for channel in [red, green, blue] {
                assert!((0.0..=1.0).contains(&channel), "{channel} is out of range");
            }
        }
    }

    /// The compositor reads exactly this many bytes, so a table of the wrong length is a protocol
    /// error rather than a wrong color.
    #[test]
    fn a_ramp_is_three_u16_channels_of_the_declared_size() {
        let size = 256;

        let file = ramp(size, 1.0, 1.0, 1.0).expect("a memfd");

        assert_eq!(
            file.metadata().expect("a size").len(),
            u64::from(size) * 3 * size_of::<u16>() as u64
        );
    }

    #[test]
    fn a_ramp_rises_to_full_scale_at_its_last_step() {
        use std::io::Read as _;
        let size = 4;

        let mut file = ramp(size, 1.0, 1.0, 1.0).expect("a memfd");
        let mut table = Vec::new();
        file.read_to_end(&mut table).expect("readable");

        let last = u16::from_ne_bytes([table[6], table[7]]);
        assert_eq!(last, u16::MAX, "the red channel ends at full scale");
    }
}
