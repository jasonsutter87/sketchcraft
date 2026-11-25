//! Small guards for handing user paths to other programs.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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

/// The size and times of a plain file (a link, a folder or a missing file gives None): what the
/// backup and side-file listings ask of every name they find.
pub fn plain_file(path: &Path) -> Option<std::fs::Metadata> {
    std::fs::symlink_metadata(path).ok().filter(|m| m.is_file())
}

/// A per-user folder for caches and work in progress that must outlive one
/// call. On Unix a shared /tmp lets other users pre-create a fixed name, so
/// it lives under the user's own ~/.cache instead (private, 0700, never a
/// symlink); elsewhere the temp folder is already per-user.
pub fn cache_dir(name: &str) -> PathBuf {
    let base = {
        #[cfg(unix)]
        {
            std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).or_else(|| if crate::live::running_as_cargo_test() { Some(std::env::temp_dir().join(format!("sketchcraft-test-cache-{}", std::process::id()))) } else { std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")) }).map(|d| d.join("sketchcraft"))
        }
        #[cfg(not(unix))]
        {
            None::<PathBuf>
        }
    };
    let Some(base) = base else {
        return std::env::temp_dir().join(format!("sketchcraft-{name}"));
    };
    let dir = base.join(name);
    // make it private; if something odd (a symlink) is in the way, use a fresh temp one
    let is_link = std::fs::symlink_metadata(&dir).map(|m| m.file_type().is_symlink()).unwrap_or(false);
    if is_link || std::fs::create_dir_all(&dir).is_err() {
        return std::env::temp_dir().join(format!("sketchcraft-{name}-{}", std::process::id()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).ok();
    }
    dir
}

/// SHA-256 of `data`, as lowercase hex (small enough to keep here rather than
/// add a dependency: used to show people what a plugin's files are).
pub fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for (i, b) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

static TMP_N: AtomicU64 = AtomicU64::new(0);

/// A unique, private scratch folder (random-ish name, created fresh, owner
/// only on Unix), removed when dropped. Replaces fixed names in a shared /tmp.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(prefix: &str) -> std::io::Result<Self> {
        let base = std::env::temp_dir();
        for _ in 0..64 {
            let n = TMP_N.fetch_add(1, Ordering::Relaxed);
            let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
            let p = base.join(format!("{prefix}-{}-{n}-{nanos:x}", std::process::id()));
            // create_new semantics: fails if anything is already there
            match std::fs::create_dir(&p) {
                Ok(()) => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).ok();
                    }
                    return Ok(TempDir(p));
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(std::io::Error::other("couldn't make a temporary folder"))
    }
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

/// What a file looked like when it was last read or written: its modification time, size and (where the system
/// has one) identity. A rename gives a file a new identity, so two saves in the same clock tick still differ.
#[derive(Clone, Copy, Debug)]
pub struct FileStamp {
    mtime: Option<std::time::SystemTime>,
    len: u64,
    id: u64,
    /// a quick fingerprint of the content (the first and last 64 KB, all of a small file): a file put back in place with
    /// its old modification time and size still looks different when its bytes are
    sig: u64,
}

/// Two stamps are the same when time, size and identity are, and the fingerprints are (a fingerprint of 0 is "couldn't be
/// read", which matches any: a file nobody can open still compares by its metadata).
impl PartialEq for FileStamp {
    fn eq(&self, o: &Self) -> bool {
        self.mtime == o.mtime && self.len == o.len && self.id == o.id && (self.sig == o.sig || self.sig == 0 || o.sig == 0)
    }
}
impl Eq for FileStamp {}

const SIG_BLOCK: usize = 64 * 1024;

/// FNV-1a over the first and last `SIG_BLOCK` bytes (and the length).
fn sig_of(first: &[u8], last: &[u8], len: u64) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325 ^ len;
    for b in first.iter().chain(last) {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// The fingerprint of whole file contents (what `file_stamp` finds when it reads the file back).
fn sig_of_bytes(bytes: &[u8]) -> u64 {
    let n = bytes.len();
    sig_of(&bytes[..n.min(SIG_BLOCK)], &bytes[n.saturating_sub(SIG_BLOCK)..], n as u64)
}

fn sig_of_file(f: &mut std::fs::File, len: u64) -> u64 {
    use std::io::{Read, Seek, SeekFrom};
    let n = len as usize;
    let mut first = vec![0u8; n.min(SIG_BLOCK)];
    let mut last = vec![0u8; n.min(SIG_BLOCK)];
    let ok = f.seek(SeekFrom::Start(0)).and_then(|_| f.read_exact(&mut first)).and_then(|_| f.seek(SeekFrom::Start(len - last.len() as u64))).and_then(|_| f.read_exact(&mut last));
    if ok.is_err() {
        return 0;
    }
    sig_of(&first, &last, len)
}

fn stamp_of(m: &std::fs::Metadata) -> FileStamp {
    #[cfg(unix)]
    let id = {
        use std::os::unix::fs::MetadataExt;
        m.ino()
    };
    #[cfg(not(unix))]
    let id = 0;
    FileStamp { mtime: m.modified().ok(), len: m.len(), id, sig: 0 }
}

/// The stamp of the file at `path` (None: no such file).
pub fn file_stamp(path: &Path) -> Option<FileStamp> {
    // a file that can't be opened (no permission) still has a stamp from its metadata, without the fingerprint
    let Ok(mut f) = std::fs::File::open(path) else { return std::fs::metadata(path).ok().map(|m| stamp_of(&m)) };
    let m = f.metadata().ok()?;
    let mut st = stamp_of(&m);
    st.sig = sig_of_file(&mut f, m.len());
    Some(st)
}

/// An advisory lock on a model file, held while a writer checks that the file is still what it read and renames its
/// new version over it (every writer of the live file takes it: the app, an agent's flush, a rollback). It lives in a
/// small hidden file beside the model and is let go when this value is dropped (or the program dies).
pub struct FileLock(#[allow(dead_code)] std::fs::File);

/// Take the lock for the model file `path`, waiting up to `wait`. Err: somebody held it that long. Ok(None): a lock
/// can't be made here (a read-only folder, a file system without locks): the stamp check still runs.
pub fn lock_for(path: &Path, wait: std::time::Duration) -> Result<Option<FileLock>, ()> {
    lock_for_why(path, wait).map(|(l, _)| l)
}

/// `lock_for`, and when no lock could be made, why (for the person: saving carries on, protected only by the stamp check).
///
/// A lock path that is not a plain file (a link, a folder, a pipe) is never followed; where the program can, it is removed
/// (a link is removed as a link, a folder only when empty) and a plain lock file made in its place. A lock file that can't
/// be opened (mode 000) is replaced the same way.
pub fn lock_for_why(path: &Path, wait: std::time::Duration) -> Result<(Option<FileLock>, Option<String>), ()> {
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
    let lock_path = dir.join(format!(".{name}.lock"));
    let open = |create_new: bool| {
        let mut o = std::fs::OpenOptions::new();
        o.read(true).write(true);
        if create_new {
            o.create_new(true);
        } else {
            o.create(true).truncate(false);
        }
        o.open(&lock_path)
    };
    // never follow a link: the lock file is ours. Something else in its place is cleared away when it can be.
    if let Ok(m) = std::fs::symlink_metadata(&lock_path) {
        if !m.file_type().is_file() {
            let gone = if m.is_dir() { std::fs::remove_dir(&lock_path) } else { std::fs::remove_file(&lock_path) };
            if gone.is_err() {
                return Ok((None, Some(format!("something that isn't a file is in the way of its lock file ({})", lock_path.display()))));
            }
        }
    }
    let f = match open(false) {
        Ok(f) => f,
        Err(e) => {
            // a lock file that can't be opened (mode 000): replaced once. (Never when the folder itself can't be written.)
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                std::fs::remove_file(&lock_path).ok();
            }
            match open(true) {
                Ok(f) => f,
                Err(e2) => return Ok((None, Some(format!("its lock file can't be made here ({e2})")))),
            }
        }
    };
    let until = std::time::Instant::now() + wait;
    loop {
        match f.try_lock() {
            Ok(()) => return Ok((Some(FileLock(f)), None)),
            Err(std::fs::TryLockError::WouldBlock) => {
                if std::time::Instant::now() >= until {
                    return Err(());
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            // a file system that has no locks (some network shares): the stamp check still runs
            Err(std::fs::TryLockError::Error(e)) => return Ok((None, Some(format!("this file system has no file locks ({e})")))),
        }
    }
}

/// What `write_atomic_guarded` did.
#[derive(Debug)]
pub enum Written {
    /// the new file is in place; this is its stamp (taken before the rename, so nobody else's later write is mistaken for it)
    Done(Option<FileStamp>),
    /// the guard said no just before the rename: the old file is untouched
    Refused,
}

/// Write a file so it is either the old one or the whole new one, never half
/// of it: write beside it, flush to disk, then rename over it.
pub fn write_atomic(path: impl AsRef<Path>, bytes: &[u8]) -> std::io::Result<()> {
    write_atomic_guarded(path, bytes, || true).map(|_| ())
}

/// `write_atomic`, but `guard` is asked as the very last step before the rename: false leaves the old file as it was.
pub fn write_atomic_guarded(path: impl AsRef<Path>, bytes: &[u8], guard: impl FnOnce() -> bool) -> std::io::Result<Written> {
    let path = path.as_ref();
    // a file that is a link is written through it (replacing the link would orphan its target)
    let target;
    let path = if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        target = std::fs::canonicalize(path)?;
        target.as_path()
    } else {
        path
    };
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
    let tmp = dir.join(format!(".{name}.tmp-{}-{}", std::process::id(), TMP_N.fetch_add(1, Ordering::Relaxed)));
    let result = (|| {
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        let stamp = f.metadata().ok().map(|m| FileStamp { sig: sig_of_bytes(bytes), ..stamp_of(&m) });
        drop(f);
        // the replacement keeps the mode the person gave the old file
        if let Ok(old) = std::fs::metadata(path) {
            std::fs::set_permissions(&tmp, old.permissions()).ok();
        }
        if !guard() {
            return Ok(Written::Refused);
        }
        std::fs::rename(&tmp, path)?;
        Ok(Written::Done(stamp))
    })();
    if !matches!(result, Ok(Written::Done(_))) {
        std::fs::remove_file(&tmp).ok();
    } else {
        #[cfg(unix)]
        if let Ok(d) = std::fs::File::open(dir) {
            d.sync_all().ok();
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn temp_dirs_are_unique_private_and_removed() {
        let (a, b) = (TempDir::new("sc-safety").unwrap(), TempDir::new("sc-safety").unwrap());
        assert_ne!(a.path(), b.path());
        assert!(a.path().is_dir());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(a.path()).unwrap().permissions().mode() & 0o777, 0o700);
        }
        let gone = a.path().to_path_buf();
        drop(a);
        assert!(!gone.exists());
    }

    #[test]
    fn atomic_writes_replace_whole_files() {
        let d = TempDir::new("sc-atomic").unwrap();
        let f = d.path().join("m.txt");
        write_atomic(&f, b"one").unwrap();
        write_atomic(&f, b"two two").unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"two two");
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1, "no temp file left behind");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o640)).unwrap();
            write_atomic(&f, b"three").unwrap();
            assert_eq!(std::fs::metadata(&f).unwrap().permissions().mode() & 0o777, 0o640);
            let link = d.path().join("link.txt");
            std::os::unix::fs::symlink(&f, &link).unwrap();
            write_atomic(&link, b"four").unwrap();
            assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
            assert_eq!(std::fs::read(&f).unwrap(), b"four");
        }
    }

    #[test]
    fn sha256_matches_known_values() {
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(sha256_hex(&[b'a'; 1000]), "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3");
    }

    #[test]
    fn dashes_are_not_flags() {
        assert_eq!(path_arg("-i evil.mp4"), PathBuf::from("./-i evil.mp4"));
        assert_eq!(path_arg("/tmp/a.mp4"), PathBuf::from("/tmp/a.mp4"));
        assert_eq!(path_arg("a-b.mp4"), PathBuf::from("a-b.mp4"));
    }
}

/// The widest or tallest picture SketchCraft decodes, the most pixels, and the most memory one decode may ask for.
/// A picture's file size says little about its decoded size (a 17 KB one-bit PNG can be 30,000 x 30,000), and the
/// model's own limits count the file, so every picture that comes from outside is measured from its header first.
///
/// The limits take what people really have (a 48 MP phone photo, a 64 MP scan, an 8,192 x 8,192 or 16,384 x 4,096
/// texture); one decode is then at most about 400 MB for a moment, and the program shrinks it straight away
/// (renders, imports and photo projection work at 1,024 to 2,048 px). A picture over the limits is skipped with a
/// message that says so (`note_skipped`), never silently.
pub const MAX_IMAGE_SIDE: u32 = 16384;
pub const MAX_IMAGE_PIXELS: u64 = 100_000_000;
pub const MAX_IMAGE_ALLOC: u64 = 640 << 20;

/// Refuse a size over the limits, with a message that names it.
pub fn check_dimensions(w: u32, h: u32) -> anyhow::Result<()> {
    if w > MAX_IMAGE_SIDE || h > MAX_IMAGE_SIDE || (w as u64) * (h as u64) > MAX_IMAGE_PIXELS {
        anyhow::bail!("that picture is {w} x {h} pixels; SketchCraft takes pictures up to {MAX_IMAGE_SIDE} on a side and {} million pixels (shrink it first)", MAX_IMAGE_PIXELS / 1_000_000);
    }
    Ok(())
}

fn reader_for(bytes: &[u8], format: Option<image::ImageFormat>) -> anyhow::Result<image::ImageReader<std::io::Cursor<&[u8]>>> {
    let mut r = image::ImageReader::new(std::io::Cursor::new(bytes));
    match format {
        Some(f) => r.set_format(f),
        None => r = r.with_guessed_format()?,
    }
    Ok(r)
}

/// The size of a picture, read from its header (nothing decoded).
pub fn image_dimensions(bytes: &[u8]) -> anyhow::Result<(u32, u32)> {
    Ok(reader_for(bytes, None)?.into_dimensions()?)
}

/// Decode a picture (PNG, JPEG, ...) after reading its size from the header: more than `MAX_IMAGE_SIDE` on a side or
/// `MAX_IMAGE_PIXELS` in all is refused before anything is allocated, and the decoder is held to `MAX_IMAGE_ALLOC`.
pub fn decode_image(bytes: &[u8], format: Option<image::ImageFormat>) -> anyhow::Result<image::DynamicImage> {
    let (w, h) = reader_for(bytes, format)?.into_dimensions()?;
    check_dimensions(w, h)?;
    let mut r = reader_for(bytes, format)?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_SIDE);
    limits.max_image_height = Some(MAX_IMAGE_SIDE);
    limits.max_alloc = Some(MAX_IMAGE_ALLOC);
    r.limits(limits);
    Ok(r.decode()?)
}

/// The size of a picture held in a `data:image/png;base64,...` (or jpeg) text, read from its header without decoding
/// the picture: the start of the text is decoded in growing pieces until the header is complete.
pub fn data_url_dimensions(data: &str) -> anyhow::Result<(u32, u32)> {
    use base64::Engine as _;
    let (head, b64) = data.split_once(";base64,").ok_or_else(|| anyhow::anyhow!("a picture must be a base64 data:image/png or data:image/jpeg URL"))?;
    if !matches!(head, "data:image/png" | "data:image/jpeg" | "data:image/jpg") {
        anyhow::bail!("a picture must be a PNG or a JPEG (data:image/png or data:image/jpeg)");
    }
    let b64 = b64.trim();
    let mut take = 8192usize;
    loop {
        let n = if take >= b64.len() { b64.len() } else { take / 4 * 4 };
        let piece = base64::engine::general_purpose::STANDARD.decode(&b64[..n]);
        if let Ok(bytes) = piece {
            if let Ok(r) = reader_for(&bytes, None) {
                if let Ok(d) = r.into_dimensions() {
                    return Ok(d);
                }
            }
        }
        if n >= b64.len() {
            anyhow::bail!("that isn't a picture the program can read (PNG or JPEG)");
        }
        take = take.saturating_mul(4);
    }
}

/// A picture about to be stored (a texture, a photo on a view): a PNG or JPEG data URL whose size is within the limits.
/// The person's window decodes whatever is stored, so the size is held at the door for every caller.
pub fn check_picture(data: &str) -> anyhow::Result<(u32, u32)> {
    let (w, h) = data_url_dimensions(data)?;
    check_dimensions(w, h)?;
    Ok((w, h))
}

/// Is this stored picture over the limits (a file from outside may hold one)? Pictures whose size can't be read are not.
pub fn picture_too_big(data: &str) -> Option<(u32, u32)> {
    let (w, h) = data_url_dimensions(data).ok()?;
    check_dimensions(w, h).is_err().then_some((w, h))
}

thread_local! {
    static SKIPPED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Note that a picture was left out of what this command is making (a render, an import), and why. The command's reply carries
/// the list (`skipped_pictures`) and the app shows it. At most 20 are kept.
pub fn note_skipped(what: &str, why: &str) {
    SKIPPED.with(|s| {
        let mut s = s.borrow_mut();
        let line = format!("{what}: {why}");
        if s.len() < 20 && !s.contains(&line) {
            s.push(line);
        }
    });
}

/// The pictures noted since the last call.
pub fn take_skipped() -> Vec<String> {
    SKIPPED.with(|s| std::mem::take(&mut *s.borrow_mut()))
}
