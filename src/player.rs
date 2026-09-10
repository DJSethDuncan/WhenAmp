use std::collections::VecDeque;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lofty::file::TaggedFileExt;
use lofty::probe::Probe;
use lofty::tag::Accessor;
use rodio::{
    stream::{DeviceSinkBuilder, MixerDeviceSink},
    ChannelCount, Decoder, SampleRate, Source,
};
use rustfft::{num_complex::Complex, Fft, FftPlanner};

/// Number of bars the visualizer displays.
pub const VISUALIZER_BARS: usize = 28;
/// Samples per FFT window (power of two). At 44.1kHz this is ~23ms of audio.
const FFT_SIZE: usize = 1024;
/// Overall visualizer gain, so bars use more of the available height.
const VISUALIZER_GAIN: f32 = 2.6;
/// Extra gain applied at the treble end (linearly ramped in from the bass
/// end, which gets 1.0x) to counteract music's natural high-frequency
/// rolloff, so bars read more evenly across the spectrum.
const VISUALIZER_TREBLE_BOOST: f32 = 4.0;
const DEFAULT_VOLUME: f32 = 0.78;
/// 0.0 = full left, 0.5 = center, 1.0 = full right.
const DEFAULT_BALANCE: f32 = 0.5;

/// Number of graphic EQ bands.
pub const NUM_EQ_BANDS: usize = 10;
/// Center frequencies (Hz) for each band — the classic Winamp 10-band
/// layout, roughly one octave apart through the bass/mids and tighter
/// through the treble where the ear is more sensitive to detail.
pub const EQ_BANDS_HZ: [f32; NUM_EQ_BANDS] =
    [60.0, 170.0, 310.0, 600.0, 1_000.0, 3_000.0, 6_000.0, 12_000.0, 14_000.0, 16_000.0];
/// Max boost/cut per band, in dB.
pub const EQ_GAIN_RANGE_DB: f32 = 12.0;
/// Filter Q (bandwidth) for each peaking band. Low enough that adjacent
/// bands overlap smoothly rather than each sounding like a narrow notch.
const EQ_Q: f32 = 1.2;

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
    balance: Arc<AtomicU32>,
    eq_enabled: Arc<AtomicBool>,
    eq_gains_db: Arc<[AtomicU32; NUM_EQ_BANDS]>,
    sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    visualizer_sample_rate: u32,
    visualizer_bars: [f32; VISUALIZER_BARS],
    fft: Arc<dyn Fft<f32>>,
}

impl Player {
    pub fn new() -> anyhow::Result<Self> {
        let device_sink = DeviceSinkBuilder::from_default_device()
            .map_err(|err| anyhow::anyhow!("{err}"))?
            .open_stream()
            .map_err(|err| anyhow::anyhow!("{err}"))?;
        let fft = FftPlanner::new().plan_fft_forward(FFT_SIZE);
        Ok(Self {
            _device_sink: device_sink,
            player: None,
            loaded_path: None,
            duration: None,
            track_title: None,
            track_info: TrackInfo::default(),
            volume: DEFAULT_VOLUME,
            balance: Arc::new(AtomicU32::new(DEFAULT_BALANCE.to_bits())),
            eq_enabled: Arc::new(AtomicBool::new(false)),
            eq_gains_db: Arc::new(std::array::from_fn(|_| AtomicU32::new(0.0f32.to_bits()))),
            sample_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(FFT_SIZE))),
            visualizer_sample_rate: 0,
            visualizer_bars: [0.0; VISUALIZER_BARS],
            fft,
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

    /// 0.0 = full left, 0.5 = center, 1.0 = full right. Applied live to
    /// whatever's currently playing (and to whatever loads next), no reload
    /// required.
    pub fn balance(&self) -> f32 {
        f32::from_bits(self.balance.load(Ordering::Relaxed))
    }

    pub fn set_balance(&mut self, balance: f32) {
        self.balance
            .store(balance.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn eq_enabled(&self) -> bool {
        self.eq_enabled.load(Ordering::Relaxed)
    }

    pub fn set_eq_enabled(&mut self, enabled: bool) {
        self.eq_enabled.store(enabled, Ordering::Relaxed);
    }

    /// Current gain (dB) for `band`, one of `0..NUM_EQ_BANDS`.
    pub fn eq_gain(&self, band: usize) -> f32 {
        self.eq_gains_db
            .get(band)
            .map(|g| f32::from_bits(g.load(Ordering::Relaxed)))
            .unwrap_or(0.0)
    }

    /// Set the gain (dB) for `band`, applied live — no reload required.
    pub fn set_eq_gain(&mut self, band: usize, db: f32) {
        if let Some(g) = self.eq_gains_db.get(band) {
            g.store(
                db.clamp(-EQ_GAIN_RANGE_DB, EQ_GAIN_RANGE_DB).to_bits(),
                Ordering::Relaxed,
            );
        }
    }

    pub fn reset_eq(&mut self) {
        for g in self.eq_gains_db.iter() {
            g.store(0.0f32.to_bits(), Ordering::Relaxed);
        }
    }

    /// A snapshot of the 28 visualizer bars (0.0..=1.0), bass on the left
    /// through treble on the right, computed via FFT over the most recent
    /// audio samples. Smooths against the previous call's result. Call once
    /// per UI frame.
    pub fn sample_visualizer(&mut self) -> [f32; VISUALIZER_BARS] {
        let is_playing = self
            .player
            .as_ref()
            .is_some_and(|player| !player.is_paused());

        let samples: Vec<f32> = if is_playing {
            let buffer = self
                .sample_buffer
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            buffer.iter().copied().collect()
        } else {
            Vec::new()
        };

        if samples.len() < FFT_SIZE / 4 || self.visualizer_sample_rate == 0 {
            for bar in &mut self.visualizer_bars {
                *bar *= 0.75;
            }
            return self.visualizer_bars;
        }

        let mut spectrum_input = vec![Complex::new(0.0_f32, 0.0); FFT_SIZE];
        let start = samples.len().saturating_sub(FFT_SIZE);
        let windowed = &samples[start..];
        let n = windowed.len();
        for (i, &sample) in windowed.iter().enumerate() {
            let window = if n > 1 {
                0.5 - 0.5 * ((std::f32::consts::TAU * i as f32) / (n as f32 - 1.0)).cos()
            } else {
                1.0
            };
            spectrum_input[i] = Complex::new(sample * window, 0.0);
        }
        self.fft.process(&mut spectrum_input);

        let bands = visualizer_band_edges(self.visualizer_sample_rate as f32, FFT_SIZE);
        let bar_count = self.visualizer_bars.len();
        for (i, (bar, &(lo, hi))) in self
            .visualizer_bars
            .iter_mut()
            .zip(bands.iter())
            .enumerate()
        {
            let hi = hi.max(lo + 1);
            let peak = spectrum_input[lo..hi]
                .iter()
                .map(|c| c.norm())
                .fold(0.0_f32, f32::max);
            // Real music rolls off toward high frequencies, so without
            // compensation the treble bars read consistently lower than
            // bass; ramp in extra gain toward the treble end to even that
            // out, on top of an overall gain so bars use more of the
            // available height. Both tuned empirically against typical
            // music levels rather than derived from a fixed reference
            // (there is no calibrated dBFS target here).
            let treble_t = i as f32 / (bar_count - 1).max(1) as f32;
            let treble_gain = 1.0 + treble_t * (VISUALIZER_TREBLE_BOOST - 1.0);
            let level = (peak / (FFT_SIZE as f32 * 0.5) * VISUALIZER_GAIN * treble_gain)
                .sqrt()
                .min(1.0);
            *bar = *bar * 0.45 + level * 0.55;
        }
        self.visualizer_bars
    }

    pub fn load(&mut self, path: PathBuf) -> anyhow::Result<()> {
        let file = File::open(&path)?;
        let byte_len = file.metadata()?.len();
        let source = Decoder::builder()
            .with_data(BufReader::new(file))
            .with_byte_len(byte_len)
            .with_seekable(true)
            .build()?;
        let duration = source.total_duration();
        let sample_rate = source.sample_rate();
        let channels = source.channels();
        let track_title = read_title_tag(&path);
        let kbps = duration
            .filter(|d| d.as_secs_f64() > 0.0)
            .and_then(|d| std::fs::metadata(&path).ok().map(|m| (m.len(), d)))
            .map(|(bytes, d)| ((bytes as f64 * 8.0) / d.as_secs_f64() / 1000.0).round() as u32);

        self.sample_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(FFT_SIZE)));
        self.visualizer_sample_rate = sample_rate.get();
        self.visualizer_bars = [0.0; VISUALIZER_BARS];
        let equalized = EqTap::new(
            source,
            channels.get() as usize,
            sample_rate.get() as f32,
            self.eq_enabled.clone(),
            self.eq_gains_db.clone(),
        );
        let tapped = VisualizerTap {
            inner: equalized,
            buffer: self.sample_buffer.clone(),
            channels: channels.get() as usize,
            frame_acc: 0.0,
            frame_pos: 0,
        };
        let panned = BalanceTap {
            inner: tapped,
            channels: channels.get() as usize,
            channel_index: 0,
            balance: self.balance.clone(),
        };

        let player = rodio::Player::connect_new(self._device_sink.mixer());
        player.append(panned);
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

/// Wraps a [`Source`], downmixing each frame to mono and feeding it into a
/// shared ring buffer as it plays, for the UI thread to run an FFT over.
/// Runs on rodio's audio thread, so buffer updates use a non-blocking lock
/// attempt to never stall playback.
struct VisualizerTap<S> {
    inner: S,
    buffer: Arc<Mutex<VecDeque<f32>>>,
    channels: usize,
    frame_acc: f32,
    frame_pos: usize,
}

impl<S: Source> Iterator for VisualizerTap<S> {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.inner.next()?;
        self.frame_acc += sample;
        self.frame_pos += 1;
        if self.frame_pos >= self.channels {
            let mono = self.frame_acc / self.channels as f32;
            self.frame_acc = 0.0;
            self.frame_pos = 0;
            if let Ok(mut buffer) = self.buffer.try_lock() {
                buffer.push_back(mono);
                while buffer.len() > FFT_SIZE {
                    buffer.pop_front();
                }
            }
        }
        Some(sample)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

/// Log-spaced FFT bin ranges (bass to treble) for each visualizer bar.
fn visualizer_band_edges(sample_rate: f32, fft_size: usize) -> [(usize, usize); VISUALIZER_BARS] {
    let nyquist = sample_rate / 2.0;
    let min_freq = (sample_rate / fft_size as f32).max(20.0);
    let max_freq = nyquist.max(min_freq * 2.0);
    let ratio = max_freq / min_freq;

    let bin_at = |bar_index: usize| -> usize {
        let t = bar_index as f32 / VISUALIZER_BARS as f32;
        let freq = min_freq * ratio.powf(t);
        (((freq * fft_size as f32) / sample_rate).round() as usize).min(fft_size / 2)
    };

    std::array::from_fn(|i| {
        let lo = bin_at(i);
        let hi = bin_at(i + 1).max(lo + 1);
        (lo, hi)
    })
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

/// Wraps a [`Source`], attenuating the left or right channel of stereo
/// audio according to a shared balance value (read live, so it can change
/// mid-playback without rebuilding the source chain). Mono sources pass
/// through unchanged, since there's nothing to pan between.
struct BalanceTap<S> {
    inner: S,
    channels: usize,
    channel_index: usize,
    balance: Arc<AtomicU32>,
}

/// Linear pan: at center both channels play at full volume; moving toward
/// one side attenuates the other channel down to silence, never boosts.
fn pan_gains(balance: f32) -> (f32, f32) {
    if balance <= 0.5 {
        (1.0, balance * 2.0)
    } else {
        ((1.0 - balance) * 2.0, 1.0)
    }
}

impl<S: Source> Iterator for BalanceTap<S> {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.inner.next()?;
        if self.channels < 2 {
            return Some(sample);
        }
        let channel = self.channel_index;
        self.channel_index = (self.channel_index + 1) % self.channels;
        let balance = f32::from_bits(self.balance.load(Ordering::Relaxed));
        let (left_gain, right_gain) = pan_gains(balance);
        let gain = match channel {
            0 => left_gain,
            1 => right_gain,
            _ => 1.0,
        };
        Some(sample * gain)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<S: Source> Source for BalanceTap<S> {
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

/// Coefficients for one RBJ "peaking EQ" biquad — boosts or cuts a band
/// around `freq` without disturbing frequencies far from it. At 0dB gain
/// this is (very close to) the identity filter.
#[derive(Clone, Copy, Default)]
struct BiquadCoeffs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl BiquadCoeffs {
    /// See the Audio EQ Cookbook (RBJ) "peakingEQ" formulas.
    fn peaking(sample_rate: f32, freq: f32, gain_db: f32, q: f32) -> Self {
        // Keep the filter well away from Nyquist so it stays stable even
        // for oddball low sample rates.
        let freq = freq.min(sample_rate * 0.45).max(1.0);
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = std::f32::consts::TAU * freq / sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * q);

        let a0 = 1.0 + alpha / a;
        Self {
            b0: (1.0 + alpha * a) / a0,
            b1: (-2.0 * cos_w0) / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: (-2.0 * cos_w0) / a0,
            a2: (1.0 - alpha / a) / a0,
        }
    }
}

/// Direct-Form-I biquad filter state (one instance per audio channel, since
/// interleaved L/R samples must not share history).
#[derive(Clone, Copy, Default)]
struct BiquadState {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl BiquadState {
    fn process(&mut self, c: &BiquadCoeffs, x: f32) -> f32 {
        let y = c.b0 * x + c.b1 * self.x1 + c.b2 * self.x2 - c.a1 * self.y1 - c.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Wraps a [`Source`] with a 10-band graphic EQ (series of peaking biquads
/// per channel). Gains and the enabled flag are read from shared atomics so
/// the UI can adjust them live, no reload required. Coefficients are only
/// recomputed when a gain actually changes.
struct EqTap<S> {
    inner: S,
    channels: usize,
    channel_index: usize,
    sample_rate: f32,
    enabled: Arc<AtomicBool>,
    gains_db: Arc<[AtomicU32; NUM_EQ_BANDS]>,
    cached_gains_db: [f32; NUM_EQ_BANDS],
    coeffs: [BiquadCoeffs; NUM_EQ_BANDS],
    // One filter state per band, per channel.
    states: Vec<[BiquadState; NUM_EQ_BANDS]>,
}

impl<S> EqTap<S> {
    fn new(
        inner: S,
        channels: usize,
        sample_rate: f32,
        enabled: Arc<AtomicBool>,
        gains_db: Arc<[AtomicU32; NUM_EQ_BANDS]>,
    ) -> Self {
        let channels = channels.max(1);
        Self {
            inner,
            channels,
            channel_index: 0,
            sample_rate,
            enabled,
            gains_db,
            cached_gains_db: [0.0; NUM_EQ_BANDS],
            coeffs: [BiquadCoeffs::default(); NUM_EQ_BANDS],
            states: vec![[BiquadState::default(); NUM_EQ_BANDS]; channels],
        }
    }
}

impl<S: Source> Iterator for EqTap<S> {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.inner.next()?;
        let channel = self.channel_index;
        self.channel_index = (self.channel_index + 1) % self.channels;

        if !self.enabled.load(Ordering::Relaxed) {
            return Some(sample);
        }

        for band in 0..NUM_EQ_BANDS {
            let gain = f32::from_bits(self.gains_db[band].load(Ordering::Relaxed));
            if gain != self.cached_gains_db[band] {
                self.cached_gains_db[band] = gain;
                self.coeffs[band] =
                    BiquadCoeffs::peaking(self.sample_rate, EQ_BANDS_HZ[band], gain, EQ_Q);
            }
        }

        let state = &mut self.states[channel];
        let mut y = sample;
        for band in 0..NUM_EQ_BANDS {
            y = state[band].process(&self.coeffs[band], y);
        }
        Some(y)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<S: Source> Source for EqTap<S> {
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
    let tag = tagged_file.primary_tag().or_else(|| tagged_file.first_tag())?;
    let title = tag.title()?;
    Some(format_track_title(&title, tag.artist().as_deref()))
}

/// "Artist - Title" when an artist tag is present and non-blank, otherwise
/// just the title.
fn format_track_title(title: &str, artist: Option<&str>) -> String {
    match artist {
        Some(artist) if !artist.trim().is_empty() => format!("{artist} - {title}"),
        _ => title.to_string(),
    }
}

/// The metadata title if present, otherwise the filename. Usable without
/// loading the file into a [`Player`] — e.g. for listing playlist entries.
pub fn track_display_name(path: &Path) -> String {
    read_title_tag(path).unwrap_or_else(|| {
        path.file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string())
    })
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

    /// Writes a single-frequency sine tone as a 16-bit mono WAV, for testing
    /// the visualizer's frequency-to-bar mapping against a known input.
    fn write_tone_wav(path: &Path, freq_hz: f32, duration_secs: f32) {
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
        for i in 0..num_samples {
            let t = i as f32 / sample_rate as f32;
            let sample = (std::f32::consts::TAU * freq_hz * t).sin();
            file.write_all(&((sample * i16::MAX as f32) as i16).to_le_bytes())
                .unwrap();
        }
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
    fn track_display_name_works_without_loading_into_a_player() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("no_tag.wav");
        write_silent_wav(&path, 1.0);

        assert_eq!(track_display_name(&path), "no_tag.wav");
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
    fn visualizer_band_edges_are_monotonic_bass_to_treble() {
        let bands = visualizer_band_edges(44100.0, FFT_SIZE);
        for pair in bands.windows(2) {
            let (_, prev_hi) = pair[0];
            let (next_lo, next_hi) = pair[1];
            assert!(next_lo >= prev_hi.saturating_sub(1), "bands overlap: {pair:?}");
            assert!(next_hi <= FFT_SIZE / 2, "band exceeds Nyquist bin: {pair:?}");
            assert!(next_lo < next_hi, "empty band: {pair:?}");
        }
    }

    #[test]
    fn visualizer_puts_bass_energy_in_the_left_bars() {
        let Some(mut player) = test_player() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bass_tone.wav");
        write_tone_wav(&path, 110.0, 2.0); // low A, clearly bass
        player.load(path).unwrap();
        player.play();
        std::thread::sleep(Duration::from_millis(200));

        let bars = player.sample_visualizer();
        let bass = bars[..4].iter().copied().fold(0.0_f32, f32::max);
        let treble = bars[VISUALIZER_BARS - 4..]
            .iter()
            .copied()
            .fold(0.0_f32, f32::max);
        assert!(
            bass > treble,
            "expected bass tone to register on the left bars: bass={bass} treble={treble} bars={bars:?}"
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
    fn seek_backward_after_playing_forward() {
        let Some(mut player) = test_player() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("silence.wav");
        write_silent_wav(&path, 5.0);
        player.load(path).unwrap();

        player.seek(Duration::from_secs_f32(3.0));
        std::thread::sleep(Duration::from_millis(50));
        let forward_pos = player.position();
        assert!(
            forward_pos >= Duration::from_millis(2800),
            "forward seek didn't land: {forward_pos:?}"
        );

        player.seek(Duration::from_secs_f32(1.0));
        std::thread::sleep(Duration::from_millis(50));
        let backward_pos = player.position();
        assert!(
            backward_pos >= Duration::from_millis(900) && backward_pos <= Duration::from_millis(1200),
            "backward seek didn't land: {backward_pos:?} (was at {forward_pos:?})"
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

    /// Runs a sine wave at `freq` through a single band's filter (settling
    /// past the transient) and returns the resulting peak amplitude.
    fn filtered_sine_peak(sample_rate: f32, band: usize, gain_db: f32, freq: f32) -> f32 {
        let coeffs = BiquadCoeffs::peaking(sample_rate, EQ_BANDS_HZ[band], gain_db, EQ_Q);
        let mut state = BiquadState::default();
        let mut peak = 0.0f32;
        let total = (sample_rate * 0.05) as usize; // 50ms
        let settle = total / 2;
        for i in 0..total {
            let t = i as f32 / sample_rate;
            let x = (std::f32::consts::TAU * freq * t).sin();
            let y = state.process(&coeffs, x);
            if i >= settle {
                peak = peak.max(y.abs());
            }
        }
        peak
    }

    #[test]
    fn peaking_eq_at_zero_db_is_unity_gain() {
        let peak = filtered_sine_peak(44_100.0, 0, 0.0, EQ_BANDS_HZ[0]);
        assert!((peak - 1.0).abs() < 0.02, "peak was {peak}, expected ~1.0");
    }

    #[test]
    fn peaking_eq_boosts_and_cuts_its_own_band() {
        let band = 4; // 1kHz
        let freq = EQ_BANDS_HZ[band];
        let boosted = filtered_sine_peak(44_100.0, band, EQ_GAIN_RANGE_DB, freq);
        let cut = filtered_sine_peak(44_100.0, band, -EQ_GAIN_RANGE_DB, freq);
        assert!(boosted > 1.5, "boosted peak was {boosted}, expected > 1.5");
        assert!(cut < 0.5, "cut peak was {cut}, expected < 0.5");
        assert!(boosted > cut);
    }

    #[test]
    fn peaking_eq_leaves_distant_frequencies_mostly_alone() {
        // Boosting the 60Hz band by the max amount shouldn't meaningfully
        // move a 16kHz tone.
        let peak = filtered_sine_peak(44_100.0, 0, EQ_GAIN_RANGE_DB, 16_000.0);
        assert!((peak - 1.0).abs() < 0.1, "peak was {peak}, expected ~1.0");
    }

    #[test]
    fn eq_enabled_and_gains_round_trip_and_persist_across_stop() {
        let Some(mut player) = test_player() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("silence.wav");
        write_silent_wav(&path, 1.0);
        player.load(path).unwrap();

        assert!(!player.eq_enabled());
        player.set_eq_enabled(true);
        assert!(player.eq_enabled());

        player.set_eq_gain(2, 20.0); // out of range, should clamp
        assert_eq!(player.eq_gain(2), EQ_GAIN_RANGE_DB);
        player.set_eq_gain(2, -3.5);
        assert!((player.eq_gain(2) - (-3.5)).abs() < 1e-6);

        player.stop();
        assert!(player.eq_enabled());
        assert!((player.eq_gain(2) - (-3.5)).abs() < 1e-6);

        player.reset_eq();
        assert_eq!(player.eq_gain(2), 0.0);
    }

    #[test]
    fn format_track_title_prefixes_the_artist_when_present() {
        assert_eq!(
            format_track_title("Static Bloom", Some("Nightjar")),
            "Nightjar - Static Bloom"
        );
        assert_eq!(format_track_title("Static Bloom", None), "Static Bloom");
        assert_eq!(format_track_title("Static Bloom", Some("   ")), "Static Bloom");
    }

    #[test]
    fn pan_gains_favor_the_louder_side_without_boosting() {
        assert_eq!(pan_gains(0.5), (1.0, 1.0));
        assert_eq!(pan_gains(0.0), (1.0, 0.0));
        assert_eq!(pan_gains(1.0), (0.0, 1.0));
        let (left, right) = pan_gains(0.25);
        assert_eq!(left, 1.0);
        assert!((right - 0.5).abs() < 1e-6);
    }

    #[test]
    fn set_balance_clamps_and_persists_across_stop() {
        let Some(mut player) = test_player() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("silence.wav");
        write_silent_wav(&path, 1.0);
        player.load(path).unwrap();

        player.set_balance(1.5);
        assert_eq!(player.balance(), 1.0);

        player.set_balance(0.2);
        player.stop();
        assert_eq!(player.balance(), 0.2);
    }
}
