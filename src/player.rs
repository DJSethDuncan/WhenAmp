use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::Duration;

use lofty::file::TaggedFileExt;
use lofty::probe::Probe;
use lofty::tag::Accessor;
use rodio::{
    stream::{DeviceSinkBuilder, MixerDeviceSink},
    Decoder, Source,
};

pub struct Player {
    _device_sink: MixerDeviceSink,
    player: Option<rodio::Player>,
    loaded_path: Option<PathBuf>,
    duration: Option<Duration>,
    track_title: Option<String>,
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
            track_title: None,
        })
    }

    pub fn loaded_path(&self) -> Option<&Path> {
        self.loaded_path.as_deref()
    }

    /// The track's metadata title if present, otherwise its filename.
    pub fn display_name(&self) -> Option<String> {
        if let Some(title) = &self.track_title {
            return Some(title.clone());
        }
        self.loaded_path.as_ref().map(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string())
        })
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
        let track_title = read_title_tag(&path);

        let player = rodio::Player::connect_new(self._device_sink.mixer());
        player.append(source);
        player.pause();

        self.player = Some(player);
        self.loaded_path = Some(path);
        self.duration = duration;
        self.track_title = track_title;
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

/// Reads the track title from the file's metadata tag, if one is present.
fn read_title_tag(path: &Path) -> Option<String> {
    let tagged_file = Probe::open(path).ok()?.read().ok()?;
    let title = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag())?
        .title()?;
    Some(title.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Writes a silent PCM WAV file so tests don't need to ship a binary fixture.
    fn write_silent_wav(path: &Path, duration_secs: f32) {
        let sample_rate: u32 = 44100;
        let channels: u16 = 1;
        let bits_per_sample: u16 = 16;
        let num_samples = (sample_rate as f32 * duration_secs) as u32;
        let data_size = num_samples * channels as u32 * (bits_per_sample as u32 / 8);
        let byte_rate = sample_rate * channels as u32 * (bits_per_sample as u32 / 8);
        let block_align = channels * (bits_per_sample / 8);
        let chunk_size = 36 + data_size;

        let mut file = File::create(path).unwrap();
        file.write_all(b"RIFF").unwrap();
        file.write_all(&chunk_size.to_le_bytes()).unwrap();
        file.write_all(b"WAVE").unwrap();
        file.write_all(b"fmt ").unwrap();
        file.write_all(&16u32.to_le_bytes()).unwrap();
        file.write_all(&1u16.to_le_bytes()).unwrap();
        file.write_all(&channels.to_le_bytes()).unwrap();
        file.write_all(&sample_rate.to_le_bytes()).unwrap();
        file.write_all(&byte_rate.to_le_bytes()).unwrap();
        file.write_all(&block_align.to_le_bytes()).unwrap();
        file.write_all(&bits_per_sample.to_le_bytes()).unwrap();
        file.write_all(b"data").unwrap();
        file.write_all(&data_size.to_le_bytes()).unwrap();
        file.write_all(&vec![0u8; data_size as usize]).unwrap();
    }

    /// Skips a test rather than failing when no audio output device is available
    /// (e.g. a headless CI runner).
    fn test_player() -> Option<Player> {
        match Player::new() {
            Ok(player) => Some(player),
            Err(err) => {
                eprintln!("skipping: no audio output device available ({err})");
                None
            }
        }
    }

    #[test]
    fn no_song_loaded_has_no_duration_and_zero_position() {
        let Some(player) = test_player() else {
            return;
        };
        assert_eq!(player.duration(), None);
        assert_eq!(player.position(), Duration::ZERO);
        assert_eq!(player.display_name(), None);
    }

    #[test]
    fn display_name_falls_back_to_filename_without_a_title_tag() {
        let Some(mut player) = test_player() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("silence.wav");
        write_silent_wav(&path, 1.0);

        player.load(path).unwrap();

        assert_eq!(player.display_name().as_deref(), Some("silence.wav"));
    }

    #[test]
    fn load_reports_duration() {
        let Some(mut player) = test_player() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("silence.wav");
        write_silent_wav(&path, 2.0);

        player.load(path.clone()).unwrap();

        assert_eq!(player.loaded_path(), Some(path.as_path()));
        let duration = player.duration().expect("wav files report a duration");
        assert!(
            (duration.as_secs_f32() - 2.0).abs() < 0.05,
            "duration was {duration:?}"
        );
    }

    #[test]
    fn seek_moves_position() {
        let Some(mut player) = test_player() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("silence.wav");
        write_silent_wav(&path, 3.0);
        player.load(path).unwrap();

        player.seek(Duration::from_secs_f32(1.5));
        std::thread::sleep(Duration::from_millis(50));

        let pos = player.position();
        assert!(
            pos >= Duration::from_millis(1400) && pos <= Duration::from_millis(1700),
            "position was {pos:?}"
        );
    }

    #[test]
    fn stop_resets_position_to_zero() {
        let Some(mut player) = test_player() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("silence.wav");
        write_silent_wav(&path, 2.0);
        player.load(path).unwrap();
        player.play();
        std::thread::sleep(Duration::from_millis(150));

        player.stop();

        assert!(
            player.position() < Duration::from_millis(50),
            "position was {:?}",
            player.position()
        );
    }
}
