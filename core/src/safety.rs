//! Small guards for handing user paths to other programs.

use std::path::{Path, PathBuf};

/// A path that can't be read as an option: other programs (ffmpeg, open,
/// the DWG converters) treat an argument starting with `-` as a flag.
pub fn path_arg(p: impl AsRef<Path>) -> PathBuf {
    let p = p.as_ref();
    if p.to_string_lossy().starts_with('-') {
        Path::new(".").join(p)
    } else {
        p.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dashes_are_not_flags() {
        assert_eq!(path_arg("-i evil.mp4"), PathBuf::from("./-i evil.mp4"));
        assert_eq!(path_arg("/tmp/a.mp4"), PathBuf::from("/tmp/a.mp4"));
        assert_eq!(path_arg("a-b.mp4"), PathBuf::from("a-b.mp4"));
    }
}
