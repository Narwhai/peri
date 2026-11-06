//! Audio output through rodio: one output stream, and a fresh `Player` per track.

use std::path::Path;
use std::time::Duration;

use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};

pub struct Engine {
    sink: MixerDeviceSink,
    player: Option<Player>,
}

/// Slider position (0–1) to amplitude: squared, so the slider feels even to the ear.
pub fn gain(volume: f32, muted: bool) -> f32 {
    if muted { 0.0 } else { volume.clamp(0.0, 1.0).powi(2) }
}

impl Engine {
    /// Opens the default output device.
    pub fn open() -> Result<Engine, String> {
        let mut sink = DeviceSinkBuilder::open_default_sink().map_err(|e| format!("Couldn't open the audio output: {e}"))?;
        sink.log_on_drop(false);
        Ok(Engine { sink, player: None })
    }

    /// Stops whatever plays and starts `path` at `gain`.
    pub fn load(&mut self, path: &Path, gain: f32) -> Result<(), String> {
        self.stop();
        let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let decoder = Decoder::try_from(file).map_err(|e| format!("Can't play {}: {e}", file_name(path)))?;
        let player = Player::connect_new(self.sink.mixer());
        player.set_volume(gain);
        player.append(decoder);
        self.player = Some(player);
        Ok(())
    }

    pub fn stop(&mut self) {
        // Dropping a player stops it.
        self.player = None;
    }

    pub fn play(&self) {
        if let Some(p) = &self.player {
            p.play();
        }
    }

    pub fn pause(&self) {
        if let Some(p) = &self.player {
            p.pause();
        }
    }

    pub fn set_gain(&self, gain: f32) {
        if let Some(p) = &self.player {
            p.set_volume(gain);
        }
    }

    pub fn seek(&self, seconds: f64) -> Result<(), String> {
        let Some(p) = &self.player else { return Ok(()) };
        p.try_seek(Duration::from_secs_f64(seconds.max(0.0))).map_err(|e| format!("Couldn't seek: {e}"))
    }

    /// Seconds into the current track.
    pub fn position(&self) -> Option<f64> {
        self.player.as_ref().map(|p| p.get_pos().as_secs_f64())
    }

    /// Whether a loaded track has played to its end.
    pub fn finished(&self) -> bool {
        self.player.as_ref().is_some_and(Player::empty)
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_is_squared_and_muting_silences() {
        assert_eq!(gain(1.0, false), 1.0);
        assert_eq!(gain(0.5, false), 0.25);
        assert_eq!(gain(0.8, true), 0.0);
        assert_eq!(gain(1.7, false), 1.0);
    }
}
