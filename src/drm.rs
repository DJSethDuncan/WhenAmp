//! Detects DRM-protected MP4 audio (FairPlay `.m4p`, CENC), which the
//! decoder can't play. DRM-free MP4/AAC files decode normally.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// Boxes on the path from the file root down to the sample descriptions.
const CONTAINERS: [&[u8; 4]; 5] = [b"moov", b"trak", b"mdia", b"minf", b"stbl"];
/// Sample entry types used for encrypted audio: `drms` is iTunes FairPlay,
/// `enca` is Common Encryption.
const PROTECTED_ENTRIES: [&[u8; 4]; 2] = [b"drms", b"enca"];
/// `stsd` is a full box (version + flags) followed by an entry count.
const STSD_PREAMBLE: u64 = 8;

/// True when `path` is an MP4 file whose audio sample entry is encrypted.
/// Non-MP4 and unreadable files return false and are left to the decoder.
pub fn is_drm_protected(path: &Path) -> bool {
    File::open(path)
        .and_then(|mut file| {
            let end = file.metadata()?.len();
            has_protected_entry(&mut file, 0, end)
        })
        .unwrap_or(false)
}

fn has_protected_entry<R: Read + Seek>(reader: &mut R, start: u64, end: u64) -> io::Result<bool> {
    let mut pos = start;
    while pos + 8 <= end {
        reader.seek(SeekFrom::Start(pos))?;
        let mut header = [0u8; 8];
        reader.read_exact(&mut header)?;
        let kind: [u8; 4] = header[4..].try_into().unwrap();
        let (size, header_len) = match u32::from_be_bytes(header[..4].try_into().unwrap()) {
            0 => (end - pos, 8),
            1 => {
                let mut large = [0u8; 8];
                reader.read_exact(&mut large)?;
                (u64::from_be_bytes(large), 16)
            }
            n => (u64::from(n), 8),
        };
        if size < header_len {
            return Ok(false);
        }
        let box_end = pos.saturating_add(size).min(end);
        let body = pos + header_len;
        let found = if PROTECTED_ENTRIES.contains(&&kind) {
            true
        } else if CONTAINERS.contains(&&kind) {
            has_protected_entry(reader, body, box_end)?
        } else if &kind == b"stsd" {
            has_protected_entry(reader, body + STSD_PREAMBLE, box_end)?
        } else {
            false
        };
        if found {
            return Ok(true);
        }
        pos = box_end;
    }
    Ok(false)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn mp4_box(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out
    }

    /// A minimal MP4 with an `ftyp` box and one audio track whose sample
    /// entry is `entry`.
    pub(crate) fn mp4_with_sample_entry(entry: &[u8; 4]) -> Vec<u8> {
        let mut stsd_body = vec![0, 0, 0, 0, 0, 0, 0, 1];
        stsd_body.extend(mp4_box(entry, &[0; 28]));
        let stbl = mp4_box(b"stbl", &mp4_box(b"stsd", &stsd_body));
        let minf = mp4_box(b"minf", &stbl);
        let mdia = mp4_box(b"mdia", &[mp4_box(b"mdhd", &[0; 24]), minf].concat());
        let moov = mp4_box(b"moov", &mp4_box(b"trak", &mdia));
        [
            mp4_box(b"ftyp", b"M4P \0\0\0\0"),
            moov,
            mp4_box(b"mdat", &[0; 16]),
        ]
        .concat()
    }

    fn write_temp(name: &str, bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    #[test]
    fn fairplay_sample_entry_is_protected() {
        let (_dir, path) = write_temp("song.m4p", &mp4_with_sample_entry(b"drms"));
        assert!(is_drm_protected(&path));
    }

    #[test]
    fn cenc_sample_entry_is_protected() {
        let (_dir, path) = write_temp("song.m4a", &mp4_with_sample_entry(b"enca"));
        assert!(is_drm_protected(&path));
    }

    #[test]
    fn plain_aac_sample_entry_is_not_protected() {
        let (_dir, path) = write_temp("song.m4p", &mp4_with_sample_entry(b"mp4a"));
        assert!(!is_drm_protected(&path));
    }

    #[test]
    fn moov_after_mdat_is_still_found() {
        let mut bytes = mp4_with_sample_entry(b"drms");
        let mdat = mp4_box(b"mdat", &[0; 64]);
        bytes.splice(16..16, mdat);
        let (_dir, path) = write_temp("song.m4p", &bytes);
        assert!(is_drm_protected(&path));
    }

    #[test]
    fn non_mp4_and_truncated_files_are_not_protected() {
        let (_dir, mp3) = write_temp("song.mp3", b"ID3\x04\0\0\0\0\0\0garbage");
        assert!(!is_drm_protected(&mp3));
        let bytes = mp4_with_sample_entry(b"drms");
        let (_dir2, truncated) = write_temp("cut.m4p", &bytes[..40]);
        assert!(!is_drm_protected(&truncated));
        assert!(!is_drm_protected(Path::new("/nonexistent/song.m4p")));
    }
}
