use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lofty::file::TaggedFileExt;
use lofty::probe::Probe;
use lofty::tag::Accessor;
use rodio::{
    stream::{DeviceSinkBuilder, MixerDeviceSink},
    ChannelCount, Decoder, SampleRate, Source,
};

/// Number of bars the visualizer displays.
pub const VISUALIZER_BARS: usize = 28;
/// How many times per second the 28 bars sweep once, when picking sample groupings.
const VISUALIZER_SWEEP_HZ: usize = 15;
const DEFAULT_VOLUME: f32 = 0.78;

/// Metadata about the currently loaded track beyond title/duration.
#[derive(Clone, Copy, Default)]
pub struct TrackInfo {
    pub sample_rate_hz: Option<u32>,
    pub channels: Option<u16>,
    pub kbps: Option<u32>,
}

pub struct Player {
    _device_sink: MixerDeviceSink,
    player: Option<rodio::Player>,
    loaded_path: Option<PathBuf>,
    duration: Option<Duration>,
    track_title: Option<String>,
    track_info: TrackInfo,
    volume: f32,
    visualizer: Arc<Mutex<[f32; VISUALIZER_BARS]>>,
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
            track_info: TrackInfo::default(),
            volume: DEFAULT_VOLUME,
            visualizer: Arc::new(Mutex::new([0.0; VISUALIZER_BARS])),
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

    pub fn track_info(&self) -> TrackInfo {
        self.track_info
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

    pub fn volume(&self) -> f32 {
        self.volume
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(player) = &self.player {
            player.set_volume(self.volume);
        }
    }

    /// A snapshot of the 28 visualizer bars (0.0..=1.0), decaying each time it's sampled.
    /// Call once per UI frame.
    pub fn sample_visualizer(&self) -> [f32; VISUALIZER_BARS] {
        let mut buckets = self
            .visualizer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let snapshot = *buckets;
        for bucket in buckets.iter_mut() {
            *bucket *= 0.6;
        }
        snapshot
    }

    pub fn load(&mut self, path: PathBuf) -> anyhow::Result<()> {
        let file = BufReader::new(File::open(&path)?);
        let source = Decoder::new(file)?;
        let duration = source.total_duration();
        let sample_rate = source.sample_rate();
        let channels = source.channels();
        let track_title = read_title_tag(&path);
        let kbps = duration
            .filter(|d| d.as_secs_f64() > 0.0)
            .and_then(|d| std::fs::metadata(&path).ok().map(|m| (m.len(), d)))
            .map(|(bytes, d)| ((bytes as f64 * 8.0) / d.as_secs_f64() / 1000.0).round() as u32);

        self.visualizer = Arc::new(Mutex::new([0.0; VISUALIZER_BARS]));
        let group_size = ((sample_rate.get() as usize * channels.get() as usize)
            / (VISUALIZER_BARS * VISUALIZER_SWEEP_HZ))
            .max(1);
        let tapped = VisualizerTap {
            inner: source,
            buckets: self.visualizer.clone(),
            counter: 0,
            group_size,
        };

        let player = rodio::Player::connect_new(self._device_sink.mixer());
        player.append(tapped);
        player.set_volume(self.volume);
        player.pause();

        self.player = Some(player);
        self.loaded_path = Some(path);
        self.duration = duration;
        self.track_title = track_title;
        self.track_info = TrackInfo {
            sample_rate_hz: Some(sample_rate.get()),
            channels: Some(channels.get()),
            kbps,
        };
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

/// Wraps a [`Source`], feeding each sample's magnitude into a shared set of
/// visualizer buckets as it plays. Runs on rodio's audio thread, so bucket
/// updates use a non-blocking lock attempt to never stall playback.
struct VisualizerTap<S> {
    inner: S,
    buckets: Arc<Mutex<[f32; VISUALIZER_BARS]>>,
    counter: usize,
    group_size: usize,
}

impl<S: Source> Iterator for VisualizerTap<S> {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.inner.next()?;
        let idx = (self.counter / self.group_size) % VISUALIZER_BARS;
        self.counter = self.counter.wrapping_add(1);
        if let Ok(mut buckets) = self.buckets.try_lock() {
            let magnitude = sample.abs();
            if magnitude > buckets[idx] {
                buckets[idx] = magnitude;
            }
        }
        Some(sample)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<S: Source> Source for VisualizerTap<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        self.inner.try_seek(pos)
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
    fn load_reports_sample_rate_and_channels() {
        let Some(mut player) = test_player() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("silence.wav");
        write_silent_wav(&path, 1.0);

        player.load(path).unwrap();

        let info = player.track_info();
        assert_eq!(info.sample_rate_hz, Some(44100));
        assert_eq!(info.channels, Some(1));
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

    #[test]
    fn set_volume_clamps_and_persists_across_stop() {
        let Some(mut player) = test_player() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("silence.wav");
        write_silent_wav(&path, 1.0);
        player.load(path).unwrap();

        player.set_volume(1.5);
        assert_eq!(player.volume(), 1.0);

        player.set_volume(0.3);
        player.stop();
        assert_eq!(player.volume(), 0.3);
    }
}
