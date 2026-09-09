use std::path::{Path, PathBuf};

use crate::player::track_display_name;

#[derive(Clone)]
pub struct PlaylistEntry {
    pub path: PathBuf,
    pub display_name: String,
}

#[derive(Default)]
pub struct Playlist {
    pub entries: Vec<PlaylistEntry>,
    pub current: Option<usize>,
}

impl Playlist {
    pub fn new() -> Self {
        Self::default()
    }

    fn make_entry(path: PathBuf) -> PlaylistEntry {
        let display_name = track_display_name(&path);
        PlaylistEntry { path, display_name }
    }

    /// Appends to the end of the queue.
    pub fn push(&mut self, path: PathBuf) {
        self.entries.push(Self::make_entry(path));
    }

    /// Inserts right after the currently playing track (or at the start if
    /// nothing is current), returning the index it landed at.
    pub fn insert_next(&mut self, path: PathBuf) -> usize {
        let index = self
            .current
            .map(|c| c + 1)
            .unwrap_or(0)
            .min(self.entries.len());
        self.entries.insert(index, Self::make_entry(path));
        index
    }

    pub fn remove(&mut self, index: usize) {
        if index >= self.entries.len() {
            return;
        }
        self.entries.remove(index);
        self.current = match self.current {
            Some(c) if index < c => Some(c - 1),
            Some(c) if index == c => None,
            other => other,
        };
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.current = None;
    }

    /// Marks `index` as the current track and returns its path, if valid.
    pub fn set_current(&mut self, index: usize) -> Option<PathBuf> {
        let entry = self.entries.get(index)?;
        self.current = Some(index);
        Some(entry.path.clone())
    }

    /// Advances to the next track and returns its path, or `None` at the end
    /// of the queue (or if the queue is empty).
    pub fn next(&mut self) -> Option<PathBuf> {
        let next_index = match self.current {
            Some(c) if c + 1 < self.entries.len() => c + 1,
            None if !self.entries.is_empty() => 0,
            _ => return None,
        };
        self.current = Some(next_index);
        Some(self.entries[next_index].path.clone())
    }

    /// Moves to the previous track and returns its path, or `None` if
    /// already at (or before) the start.
    pub fn prev(&mut self) -> Option<PathBuf> {
        let prev_index = match self.current {
            Some(c) if c > 0 => c - 1,
            _ => return None,
        };
        self.current = Some(prev_index);
        Some(self.entries[prev_index].path.clone())
    }

    /// Writes the queue as an M3U8 playlist (UTF-8, one path per line, with
    /// `#EXTINF` display-name hints) — the most broadly supported playlist
    /// format across players.
    pub fn save_m3u(&self, path: &Path) -> std::io::Result<()> {
        let mut out = String::from("#EXTM3U\n");
        for entry in &self.entries {
            out.push_str("#EXTINF:-1,");
            out.push_str(&entry.display_name);
            out.push('\n');
            out.push_str(&entry.path.to_string_lossy());
            out.push('\n');
        }
        std::fs::write(path, out)
    }

    /// Reads an M3U/M3U8 playlist file and returns the track paths it lists.
    /// Relative paths are resolved against the playlist file's own
    /// directory, matching how other players interpret them.
    pub fn load_m3u(path: &Path) -> std::io::Result<Vec<PathBuf>> {
        let content = std::fs::read_to_string(path)?;
        let base_dir = path.parent().map(Path::to_path_buf);
        let mut paths = Vec::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let candidate = PathBuf::from(line);
            let resolved = if candidate.is_absolute() {
                candidate
            } else {
                base_dir
                    .as_ref()
                    .map(|base| base.join(&candidate))
                    .unwrap_or(candidate)
            };
            paths.push(resolved);
        }
        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"").unwrap();
        path
    }

    #[test]
    fn insert_next_lands_right_after_current() {
        let dir = tempfile::tempdir().unwrap();
        let mut playlist = Playlist::new();
        playlist.push(touch(dir.path(), "a.mp3"));
        playlist.push(touch(dir.path(), "b.mp3"));
        playlist.current = Some(0);

        let index = playlist.insert_next(touch(dir.path(), "c.mp3"));

        assert_eq!(index, 1);
        assert_eq!(playlist.entries[1].display_name, "c.mp3");
        assert_eq!(playlist.entries[2].display_name, "b.mp3");
    }

    #[test]
    fn insert_next_with_no_current_goes_to_the_front() {
        let dir = tempfile::tempdir().unwrap();
        let mut playlist = Playlist::new();
        playlist.push(touch(dir.path(), "a.mp3"));

        let index = playlist.insert_next(touch(dir.path(), "b.mp3"));

        assert_eq!(index, 0);
        assert_eq!(playlist.entries[0].display_name, "b.mp3");
    }

    #[test]
    fn next_and_prev_walk_the_queue_and_stop_at_the_ends() {
        let dir = tempfile::tempdir().unwrap();
        let mut playlist = Playlist::new();
        playlist.push(touch(dir.path(), "a.mp3"));
        playlist.push(touch(dir.path(), "b.mp3"));

        assert_eq!(playlist.current, None);
        assert!(playlist.next().is_some()); // -> index 0
        assert_eq!(playlist.current, Some(0));
        assert!(playlist.next().is_some()); // -> index 1
        assert_eq!(playlist.current, Some(1));
        assert!(playlist.next().is_none()); // already at the end
        assert_eq!(playlist.current, Some(1));

        assert!(playlist.prev().is_some()); // -> index 0
        assert_eq!(playlist.current, Some(0));
        assert!(playlist.prev().is_none()); // already at the start
        assert_eq!(playlist.current, Some(0));
    }

    #[test]
    fn remove_before_current_shifts_the_index_down() {
        let dir = tempfile::tempdir().unwrap();
        let mut playlist = Playlist::new();
        playlist.push(touch(dir.path(), "a.mp3"));
        playlist.push(touch(dir.path(), "b.mp3"));
        playlist.push(touch(dir.path(), "c.mp3"));
        playlist.current = Some(2);

        playlist.remove(0);

        assert_eq!(playlist.current, Some(1));
        assert_eq!(playlist.entries[1].display_name, "c.mp3");
    }

    #[test]
    fn remove_current_clears_current() {
        let dir = tempfile::tempdir().unwrap();
        let mut playlist = Playlist::new();
        playlist.push(touch(dir.path(), "a.mp3"));
        playlist.current = Some(0);

        playlist.remove(0);

        assert_eq!(playlist.current, None);
        assert!(playlist.entries.is_empty());
    }

    #[test]
    fn m3u_round_trip_preserves_paths() {
        let dir = tempfile::tempdir().unwrap();
        let mut playlist = Playlist::new();
        playlist.push(touch(dir.path(), "a.mp3"));
        playlist.push(touch(dir.path(), "b.mp3"));

        let m3u_path = dir.path().join("list.m3u8");
        playlist.save_m3u(&m3u_path).unwrap();

        let loaded = Playlist::load_m3u(&m3u_path).unwrap();

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0], playlist.entries[0].path);
        assert_eq!(loaded[1], playlist.entries[1].path);
    }

    #[test]
    fn m3u_load_resolves_relative_paths_against_the_playlist_dir() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "song.mp3");
        let m3u_path = dir.path().join("list.m3u");
        std::fs::write(&m3u_path, "#EXTM3U\nsong.mp3\n").unwrap();

        let loaded = Playlist::load_m3u(&m3u_path).unwrap();

        assert_eq!(loaded, vec![dir.path().join("song.mp3")]);
    }
}
