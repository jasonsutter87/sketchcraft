// Shared mutation fuzzer (included by the fuzz tests with #[path]).
#![allow(dead_code)]

pub struct Rng(pub u64);
impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
}

const INTERESTING: [&[u8]; 10] = [b"0", b"-1", b"999999999", b"1e308", b"NaN", b"inf", b"-0", b"4294967295", b"\xff\xff\xff\xff", b"\x00\x00\x00\x80"];

pub fn mutate(r: &mut Rng, seed: &[u8], text: bool) -> Vec<u8> {
    let mut d = seed.to_vec();
    for _ in 0..1 + r.below(6) {
        if d.is_empty() {
            d.push(0);
        }
        match r.below(8) {
            0 => { let i = r.below(d.len()); d[i] ^= 1 << r.below(8); }
            1 => { let i = r.below(d.len()); d[i] = r.next() as u8; }
            2 => { let i = r.below(d.len()); let v = INTERESTING[r.below(INTERESTING.len())]; d.splice(i..(i + v.len()).min(d.len()), v.iter().copied()); }
            3 => { let i = r.below(d.len()); let n = r.below(64).min(d.len() - i); d.drain(i..i + n); }
            4 => { let i = r.below(d.len()); let n = r.below(64).min(d.len() - i); let chunk: Vec<u8> = d[i..i + n].to_vec(); let j = r.below(d.len()); d.splice(j..j, chunk); }
            5 => { let n = r.below(d.len()); d.truncate(n.max(1)); }
            6 if text => {
                // swap a number for a nasty one
                let nums: Vec<(usize, usize)> = d.iter().enumerate().filter(|(_, c)| c.is_ascii_digit()).map(|(i, _)| (i, i + 1)).collect();
                if let Some(&(a, b)) = nums.get(r.below(nums.len().max(1))) {
                    let v = INTERESTING[r.below(7)];
                    d.splice(a..b, v.iter().copied());
                }
            }
            _ => { let i = r.below(d.len()); let v = r.next().to_le_bytes(); d.splice(i..i, v.iter().take(1 + r.below(8)).copied()); }
        }
    }
    d
}

/// Run `f` on `iters` mutants of `seed`; returns (panics, slowest ms). Panicking inputs are saved.
pub fn fuzz(name: &str, seed: &[u8], text: bool, iters: usize, f: impl Fn(&[u8]) + std::panic::RefUnwindSafe) -> (usize, u128) {
    std::panic::set_hook(Box::new(|_| {}));
    let mut r = Rng(0x9E3779B97F4A7C15 ^ name.len() as u64 * 0x1234567);
    let (mut panics, mut slowest) = (0, 0u128);
    for i in 0..iters {
        let d = mutate(&mut r, seed, text);
        // a crash that can't be caught (stack overflow, abort) leaves its input here
        if std::env::var_os("FUZZ_TRACE").is_some() {
            std::fs::write(format!("../out/fuzz/{name}-current.bin"), &d).ok();
        }
        let t = std::time::Instant::now();
        let ok = std::panic::catch_unwind(|| f(&d)).is_ok();
        let ms = t.elapsed().as_millis();
        if ms > slowest {
            slowest = ms;
            if ms > 2000 {
                std::fs::write(format!("../out/fuzz/{name}-slow-{i}.bin"), &d).ok();
            }
        }
        if !ok {
            panics += 1;
            if panics <= 20 {
                std::fs::write(format!("../out/fuzz/{name}-panic-{i}.bin"), &d).ok();
            }
        }
    }
    let _ = std::panic::take_hook();
    (panics, slowest)
}
