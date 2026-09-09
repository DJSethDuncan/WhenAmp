use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rodio::{
    stream::{DeviceSinkBuilder, MixerDeviceSink},
    Decoder, Source,
};

pub struct Player {
    _device_sink: MixerDeviceSink,
    player: Option<rodio::Player>,
    loaded_path: Option<PathBuf>,
    duration: Option<Duration>,
}

impl Player {
    pub fn new() -> anyhow::Result<Self> {
        let device_sink = DeviceSinkBuilder::from_default_device()
            .map_err(|err| anyhow::anyhow!("{err}"))?
            .open_stream()
            .map_err(|err| anyhow::anyhow!("{err}"))?;
        Ok(Self {
            _device_sink: device_sink,
            player: None,
            loaded_path: None,
            duration: None,
        })
    }

    pub fn loaded_path(&self) -> Option<&Path> {
        self.loaded_path.as_deref()
    }

    pub fn duration(&self) -> Option<Duration> {
        self.duration
    }

    pub fn position(&self) -> Duration {
        self.player
            .as_ref()
            .map(|player| player.get_pos())
            .unwrap_or_default()
    }

    pub fn seek(&self, pos: Duration) {
        if let Some(player) = &self.player {
            let _ = player.try_seek(pos);
        }
    }

    pub fn load(&mut self, path: PathBuf) -> anyhow::Result<()> {
        let file = BufReader::new(File::open(&path)?);
        let source = Decoder::new(file)?;
        let duration = source.total_duration();

        let player = rodio::Player::connect_new(self._device_sink.mixer());
        player.append(source);
        player.pause();

        self.player = Some(player);
        self.loaded_path = Some(path);
        self.duration = duration;
        Ok(())
    }

    pub fn play(&self) {
        if let Some(player) = &self.player {
            player.play();
        }
    }

    pub fn pause(&self) {
        if let Some(player) = &self.player {
            player.pause();
        }
    }

    /// Stops playback and resets to the beginning of the loaded track.
    pub fn stop(&mut self) {
        self.player = None;
        if let Some(path) = self.loaded_path.clone() {
            let _ = self.load(path);
        }
    }
}
