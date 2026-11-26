//! 3D mouse (3Dconnexion SpaceMouse) input, read straight from the device. No 3Dconnexion SDK is bundled or needed.
//!
//! A SpaceMouse is a USB (or radio) HID device that reports a six-degrees-of-freedom push/twist and a few buttons.
//! This module finds one (vendor 0x256f, or the older Logitech-era 0x046d ids), decodes its reports into a `Frame`
//! and sends each changed frame to the page as the `spacemouse` event. What the page does with it (orbit, pan,
//! zoom, settings) is in `app/src/spacemouse.ts`. On Linux, when `spacenavd` is running, its socket is used
//! instead of the raw device, because the daemon owns the device there.
//!
//! The decoding (`Decoder`, `parse_spnav`) and the read loop (`pump`) take their bytes from a `ReportSource`, so
//! they are tested here with recorded and synthetic reports and no hardware. Opening the real device is the only
//! part that needs one.

// the spacenavd socket is Linux-only; its parser still builds (and is tested) everywhere
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// 3Dconnexion's USB vendor id since 2013 (every current SpaceMouse).
pub const VENDOR_3DCONNEXION: u16 = 0x256f;
/// The vendor id older devices (SpaceNavigator, SpaceExplorer, SpacePilot, SpaceMouse Pro) carry: Logitech's.
pub const VENDOR_LOGITECH: u16 = 0x046d;
/// Logitech's id is shared with mice and keyboards, so only the 3D mouse product ids in this range count.
const LOGITECH_3D_PRODUCTS: std::ops::RangeInclusive<u16> = 0xc603..=0xc62b;

/// Is this HID interface a 3D mouse? (`usage_page` 1 is "generic desktop"; the SpaceMouse reports usage 8,
/// "multi-axis controller", and a few older firmwares say 4, "joystick".)
pub fn is_spacemouse(vendor: u16, product: u16, usage_page: u16, usage: u16) -> bool {
    let usage_ok = usage_page == 0 || (usage_page == 1 && (usage == 8 || usage == 4));
    match vendor {
        VENDOR_3DCONNEXION => usage_ok,
        VENDOR_LOGITECH => usage_ok && LOGITECH_3D_PRODUCTS.contains(&product),
        _ => false,
    }
}

/// One reading of the device: how far the cap is pushed (`t`) and twisted (`r`), raw (about -350..350 at full
/// deflection), and which buttons are down (bit n is button n).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Frame {
    pub t: [i32; 3],
    pub r: [i32; 3],
    pub buttons: u32,
}

/// Turns the device's HID reports into frames. Translation and rotation arrive in separate reports on most
/// models, so the decoder keeps the latest of each.
#[derive(Default)]
pub struct Decoder {
    frame: Frame,
}

fn i16_at(b: &[u8], at: usize) -> i32 {
    i16::from_le_bytes([b[at], b[at + 1]]) as i32
}

impl Decoder {
    /// Feed one HID report (report id first). Returns the new frame when it differs from the last one.
    ///  id 1: translation x, y, z as three little-endian i16 (newer models add rotation after it, 13 bytes)
    ///  id 2: rotation rx, ry, rz as three little-endian i16
    ///  id 3: buttons, one bit each, little-endian
    pub fn feed(&mut self, report: &[u8]) -> Option<Frame> {
        let before = self.frame;
        match report.first().copied() {
            Some(1) if report.len() >= 7 => {
                self.frame.t = [i16_at(report, 1), i16_at(report, 3), i16_at(report, 5)];
                if report.len() >= 13 {
                    self.frame.r = [i16_at(report, 7), i16_at(report, 9), i16_at(report, 11)];
                }
            }
            Some(2) if report.len() >= 7 => self.frame.r = [i16_at(report, 1), i16_at(report, 3), i16_at(report, 5)],
            Some(3) if report.len() >= 2 => {
                let mut v = 0u32;
                for (i, b) in report[1..].iter().take(4).enumerate() {
                    v |= (*b as u32) << (8 * i);
                }
                self.frame.buttons = v;
            }
            _ => {}
        }
        (self.frame != before).then_some(self.frame)
    }

    /// A spacenavd event (`parse_spnav`) applied to the frame.
    pub fn feed_spnav(&mut self, ev: Spnav) -> Option<Frame> {
        let before = self.frame;
        match ev {
            Spnav::Motion { t, r } => {
                self.frame.t = t;
                self.frame.r = r;
            }
            Spnav::Button { bnum, press } => {
                if bnum < 32 {
                    if press {
                        self.frame.buttons |= 1 << bnum;
                    } else {
                        self.frame.buttons &= !(1 << bnum);
                    }
                }
            }
        }
        (self.frame != before).then_some(self.frame)
    }

    /// The device went away: everything lets go.
    pub fn reset(&mut self) -> Option<Frame> {
        let before = self.frame;
        self.frame = Frame::default();
        (self.frame != before).then_some(self.frame)
    }
}

/// An event from spacenavd's socket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spnav {
    Motion { t: [i32; 3], r: [i32; 3] },
    Button { bnum: u32, press: bool },
}

/// spacenavd (libspnav's wire format) sends eight native-endian 32-bit integers per event: the type (0 motion,
/// 1 button), then for a motion x, y, z, rx, ry, rz and the period in ms, or for a button the press flag and the
/// button number.
pub fn parse_spnav(b: &[u8; 32]) -> Option<Spnav> {
    let d: Vec<i32> = b.chunks_exact(4).map(|c| i32::from_ne_bytes([c[0], c[1], c[2], c[3]])).collect();
    match d[0] {
        0 => Some(Spnav::Motion { t: [d[1], d[2], d[3]], r: [d[4], d[5], d[6]] }),
        1 if d[2] >= 0 => Some(Spnav::Button { bnum: d[2] as u32, press: d[1] != 0 }),
        _ => None,
    }
}

/// Where reports come from. The real ones are a HID handle and the spacenavd socket; tests use a list.
pub trait ReportSource {
    /// Wait up to `timeout` for the next report. `Ok(None)` is a quiet moment, `Err` means the device is gone.
    fn next(&mut self, timeout: Duration) -> Result<Option<Input>, String>;
}

/// What a source hands over.
pub enum Input {
    Hid(Vec<u8>),
    Spnav(Spnav),
}

/// Read from `source` until it fails or `keep_going` says stop, calling `emit` for every changed frame. On the way
/// out everything is let go (a final zero frame if anything was held), so a pulled cable never leaves the camera
/// drifting. Returns why it stopped.
pub fn pump(source: &mut dyn ReportSource, keep_going: &dyn Fn() -> bool, emit: &mut dyn FnMut(Frame)) -> String {
    let mut dec = Decoder::default();
    let why = loop {
        if !keep_going() {
            break "stopped".to_string();
        }
        match source.next(Duration::from_millis(250)) {
            Ok(Some(Input::Hid(bytes))) => {
                if let Some(f) = dec.feed(&bytes) {
                    emit(f)
                }
            }
            Ok(Some(Input::Spnav(ev))) => {
                if let Some(f) = dec.feed_spnav(ev) {
                    emit(f)
                }
            }
            Ok(None) => {}
            Err(e) => break e,
        }
    };
    if let Some(f) = dec.reset() {
        emit(f);
    }
    why
}

// ---- the real device ----

/// What the page is told about the device.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Status {
    pub connected: bool,
    /// the product name the device reports ("SpaceMouse Compact"), or "spacenavd"
    pub name: String,
    /// "hid" (read from the device) or "spacenavd" (through the Linux daemon)
    pub via: String,
}

static STATUS: Mutex<Option<Status>> = Mutex::new(None);
static ENABLED: AtomicBool = AtomicBool::new(false);
static STARTED: AtomicBool = AtomicBool::new(false);

pub fn status() -> Status {
    STATUS.lock().unwrap_or_else(|p| p.into_inner()).clone().unwrap_or_default()
}

fn set_status(app: &tauri::AppHandle, s: Status) {
    use tauri::Emitter;
    *STATUS.lock().unwrap_or_else(|p| p.into_inner()) = Some(s.clone());
    let _ = app.emit("spacemouse-status", s);
}

/// Turn the 3D mouse on or off (the page's setting). The first "on" starts the watcher thread; "off" makes it let
/// go of the device.
pub fn enable(app: &tauri::AppHandle, on: bool) {
    ENABLED.store(on, Ordering::SeqCst);
    if on && !STARTED.swap(true, Ordering::SeqCst) {
        let app = app.clone();
        std::thread::spawn(move || watch(app));
    }
}

fn watch(app: tauri::AppHandle) {
    use tauri::Emitter;
    let mut api = hidapi::HidApi::new().ok();
    loop {
        if !ENABLED.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(500));
            continue;
        }
        let mut emit = |f: Frame| {
            let _ = app.emit("spacemouse", f);
        };
        let keep = || ENABLED.load(Ordering::SeqCst);
        let mut ran = false;

        #[cfg(target_os = "linux")]
        if let Some(mut s) = spnav::open() {
            ran = true;
            set_status(&app, Status { connected: true, name: "spacenavd".into(), via: "spacenavd".into() });
            pump(&mut s, &keep, &mut emit);
        }

        if !ran {
            if let Some(h) = api.as_mut() {
                let _ = h.refresh_devices();
                let found = h.device_list().find(|d| is_spacemouse(d.vendor_id(), d.product_id(), d.usage_page(), d.usage())).map(|d| (d.path().to_owned(), d.product_string().unwrap_or("3D mouse").to_string()));
                if let Some((path, name)) = found {
                    if let Ok(dev) = h.open_path(&path) {
                        ran = true;
                        set_status(&app, Status { connected: true, name, via: "hid".into() });
                        pump(&mut Hid(dev), &keep, &mut emit);
                    }
                }
            } else {
                api = hidapi::HidApi::new().ok();
            }
        }
        if ran {
            set_status(&app, Status::default());
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

struct Hid(hidapi::HidDevice);

impl ReportSource for Hid {
    fn next(&mut self, timeout: Duration) -> Result<Option<Input>, String> {
        let mut buf = [0u8; 64];
        match self.0.read_timeout(&mut buf, timeout.as_millis() as i32) {
            Ok(0) => Ok(None),
            Ok(n) => Ok(Some(Input::Hid(buf[..n].to_vec()))),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// Only a socket owned by root (the system daemon) or by the person running the app is trusted: another
/// user could create `/tmp/.spnav.sock` and steer the camera.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn trusted_owner(owner: u32, me: u32) -> bool {
    owner == 0 || owner == me
}

/// Read toward a full 32-byte event, keeping what has arrived across timeouts so a slow sender never
/// shifts the framing. `Ok(true)` once `buf` holds a whole event (and `have` is reset), `Ok(false)` on a timeout.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn fill_frame<R: std::io::Read>(r: &mut R, buf: &mut [u8; 32], have: &mut usize) -> std::io::Result<bool> {
    while *have < buf.len() {
        match r.read(&mut buf[*have..]) {
            Ok(0) => return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "the 3D mouse driver closed the connection")),
            Ok(n) => *have += n,
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => return Ok(false),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    *have = 0;
    Ok(true)
}

#[cfg(target_os = "linux")]
mod spnav {
    use super::*;
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    use std::os::unix::net::UnixStream;

    pub struct Sock(UnixStream, [u8; 32], usize);

    /// spacenavd listens on /var/run/spnav.sock (older builds: /tmp/.spnav.sock). Only a socket
    /// owned by root or by this user is used.
    pub fn open() -> Option<Sock> {
        let me = std::fs::metadata("/proc/self").map(|m| m.uid()).ok()?;
        for path in ["/var/run/spnav.sock", "/run/spnav.sock", "/tmp/.spnav.sock"] {
            let Ok(m) = std::fs::symlink_metadata(path) else { continue };
            if !m.file_type().is_socket() || !trusted_owner(m.uid(), me) {
                continue;
            }
            if let Ok(s) = UnixStream::connect(path) {
                return Some(Sock(s, [0; 32], 0));
            }
        }
        None
    }

    impl ReportSource for Sock {
        fn next(&mut self, timeout: Duration) -> Result<Option<Input>, String> {
            let _ = self.0.set_read_timeout(Some(timeout));
            let Sock(stream, buf, have) = self;
            match fill_frame(stream, buf, have) {
                Ok(true) => Ok(parse_spnav(buf).map(Input::Spnav)),
                Ok(false) => Ok(None),
                Err(e) => Err(e.to_string()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    /// A report as the device sends it: id, then little-endian i16 values.
    fn rep(id: u8, vals: &[i16]) -> Vec<u8> {
        let mut v = vec![id];
        for x in vals {
            v.extend(x.to_le_bytes());
        }
        v
    }

    #[test]
    fn recognizes_3d_mice_and_not_other_logitech_devices() {
        assert!(is_spacemouse(0x256f, 0xc635, 1, 8)); // SpaceMouse Compact
        assert!(is_spacemouse(0x256f, 0xc62e, 0, 0)); // usage not reported (some platforms)
        assert!(is_spacemouse(0x046d, 0xc626, 1, 8)); // SpaceNavigator
        assert!(is_spacemouse(0x046d, 0xc62b, 1, 8)); // SpaceMouse Pro
        assert!(!is_spacemouse(0x046d, 0xc52b, 1, 8)); // a Logitech wireless receiver
        assert!(!is_spacemouse(0x046d, 0xc077, 1, 2)); // a Logitech mouse
        assert!(!is_spacemouse(0x256f, 0xc635, 12, 1)); // a different interface (consumer control) of the device
        assert!(!is_spacemouse(0x1234, 0xc626, 1, 8));
    }

    #[test]
    fn decodes_translation_rotation_and_buttons() {
        let mut d = Decoder::default();
        let f = d.feed(&rep(1, &[100, -200, 350])).unwrap();
        assert_eq!(f.t, [100, -200, 350]);
        assert_eq!(f.r, [0, 0, 0]);
        let f = d.feed(&rep(2, &[-5, 6, -7])).unwrap();
        assert_eq!((f.t, f.r), ([100, -200, 350], [-5, 6, -7]));
        let f = d.feed(&[3, 0b0000_0101]).unwrap();
        assert_eq!(f.buttons, 0b101);
        let f = d.feed(&[3, 0, 0b10]).unwrap(); // a button in the second byte (button 9)
        assert_eq!(f.buttons, 1 << 9);
    }

    #[test]
    fn newer_models_send_both_in_one_report() {
        let mut d = Decoder::default();
        let f = d.feed(&rep(1, &[1, 2, 3, 4, 5, 6])).unwrap();
        assert_eq!((f.t, f.r), ([1, 2, 3], [4, 5, 6]));
    }

    #[test]
    fn repeats_and_junk_change_nothing() {
        let mut d = Decoder::default();
        assert!(d.feed(&rep(1, &[10, 0, 0])).is_some());
        assert!(d.feed(&rep(1, &[10, 0, 0])).is_none());
        assert!(d.feed(&[]).is_none());
        assert!(d.feed(&[1, 1, 2]).is_none()); // too short
        assert!(d.feed(&[9, 1, 2, 3, 4, 5, 6]).is_none()); // an id we don't know
        assert!(d.feed(&[3]).is_none());
        assert_eq!(d.feed(&rep(1, &[0, 0, 0])).unwrap().t, [0, 0, 0]); // letting go is a change
    }

    #[test]
    fn only_root_or_me_may_own_the_socket() {
        assert!(trusted_owner(0, 1000));
        assert!(trusted_owner(1000, 1000));
        assert!(!trusted_owner(1001, 1000));
    }

    #[test]
    fn a_slow_sender_does_not_break_the_framing() {
        // a reader that hands out 10 bytes, times out, then the rest
        struct Slow(Vec<Result<Vec<u8>, std::io::ErrorKind>>);
        impl std::io::Read for Slow {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                match self.0.remove(0) {
                    Ok(b) => {
                        out[..b.len()].copy_from_slice(&b);
                        Ok(b.len())
                    }
                    Err(k) => Err(k.into()),
                }
            }
        }
        let data: Vec<u8> = (0..32).collect();
        let mut r = Slow(vec![Ok(data[..10].to_vec()), Err(std::io::ErrorKind::TimedOut), Ok(data[10..].to_vec())]);
        let (mut buf, mut have) = ([0u8; 32], 0usize);
        assert!(!fill_frame(&mut r, &mut buf, &mut have).unwrap());
        assert_eq!(have, 10);
        assert!(fill_frame(&mut r, &mut buf, &mut have).unwrap());
        assert_eq!(have, 0);
        assert_eq!(buf.to_vec(), data);
        let mut closed = Slow(vec![Ok(vec![])]);
        assert!(fill_frame(&mut closed, &mut buf, &mut have).is_err());
    }

    #[test]
    fn spacenavd_events() {
        let ev = |data: [i32; 8]| {
            let mut b = [0u8; 32];
            for (i, v) in data.iter().enumerate() {
                b[i * 4..i * 4 + 4].copy_from_slice(&v.to_ne_bytes());
            }
            b
        };
        assert_eq!(parse_spnav(&ev([0, 1, 2, 3, 4, 5, 6, 16])), Some(Spnav::Motion { t: [1, 2, 3], r: [4, 5, 6] }));
        assert_eq!(parse_spnav(&ev([1, 1, 3, 0, 0, 0, 0, 0])), Some(Spnav::Button { bnum: 3, press: true }));
        assert_eq!(parse_spnav(&ev([1, 0, 3, 0, 0, 0, 0, 0])), Some(Spnav::Button { bnum: 3, press: false }));
        assert_eq!(parse_spnav(&ev([7, 0, 0, 0, 0, 0, 0, 0])), None);
        assert_eq!(parse_spnav(&ev([1, 1, -1, 0, 0, 0, 0, 0])), None);
        let mut d = Decoder::default();
        d.feed_spnav(Spnav::Button { bnum: 1, press: true });
        d.feed_spnav(Spnav::Button { bnum: 4, press: true });
        let f = d.feed_spnav(Spnav::Button { bnum: 1, press: false }).unwrap();
        assert_eq!(f.buttons, 1 << 4);
    }

    struct Script(RefCell<VecDeque<Result<Option<Input>, String>>>);
    impl ReportSource for Script {
        fn next(&mut self, _: Duration) -> Result<Option<Input>, String> {
            self.0.borrow_mut().pop_front().unwrap_or(Err("end".into()))
        }
    }

    #[test]
    fn the_read_loop_emits_changes_and_lets_go_when_the_device_goes() {
        let mut src = Script(RefCell::new(VecDeque::from(vec![
            Ok(Some(Input::Hid(rep(1, &[50, 0, 0])))),
            Ok(None),
            Ok(Some(Input::Hid(rep(1, &[50, 0, 0])))), // a repeat
            Ok(Some(Input::Hid(rep(2, &[0, 0, 80])))),
            Err("device unplugged".into()),
        ])));
        let mut got = vec![];
        let why = pump(&mut src, &|| true, &mut |f| got.push(f));
        assert_eq!(why, "device unplugged");
        assert_eq!(got.len(), 3);
        assert_eq!(got[1].r, [0, 0, 80]);
        assert_eq!(got[2], Frame::default(), "a pulled cable stops the camera");
    }

    #[test]
    fn the_read_loop_stops_when_turned_off() {
        let mut src = Script(RefCell::new(VecDeque::from(vec![Ok(Some(Input::Hid(rep(1, &[5, 5, 5]))))])));
        let mut n = 0;
        let why = pump(&mut src, &|| false, &mut |_| n += 1);
        assert_eq!((why.as_str(), n), ("stopped", 0));
    }
}
