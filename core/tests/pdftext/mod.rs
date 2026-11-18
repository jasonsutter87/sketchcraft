//! Just enough PDF reading for tests: objects, page content streams, the text a page draws
//! (through each font's ToUnicode map) and the line widths it strokes with.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::io::Read;

pub struct Obj {
    pub dict: String,
    pub stream: Option<Vec<u8>>,
}

pub struct Pdf {
    pub objs: BTreeMap<u32, Obj>,
}

fn find(h: &[u8], n: &[u8], from: usize) -> Option<usize> {
    h.get(from..)?.windows(n.len()).position(|w| w == n).map(|p| p + from)
}

pub fn parse(bytes: &[u8]) -> Pdf {
    let mut objs = BTreeMap::new();
    let mut at = 0;
    while let Some(p) = find(bytes, b" 0 obj\n", at) {
        let mut s = p;
        while s > 0 && bytes[s - 1].is_ascii_digit() {
            s -= 1;
        }
        let id: u32 = String::from_utf8_lossy(&bytes[s..p]).parse().unwrap_or(0);
        let body = p + 7;
        let dict_end = find(bytes, b"stream\n", body).filter(|e| find(bytes, b"endobj", body).is_none_or(|o| *e < o));
        let (dict, stream, next) = match dict_end {
            Some(e) => {
                let dict = String::from_utf8_lossy(&bytes[body..e]).to_string();
                let len: usize = dict.split("/Length").nth(1).and_then(|t| t.split_whitespace().next()).and_then(|t| t.parse().ok()).unwrap_or(0);
                let raw = bytes[e + 7..e + 7 + len].to_vec();
                let data = if dict.contains("/FlateDecode") {
                    let mut out = Vec::new();
                    flate2::read::ZlibDecoder::new(&raw[..]).read_to_end(&mut out).expect("a stream inflates");
                    out
                } else {
                    raw
                };
                (dict, Some(data), e + 7 + len)
            }
            None => {
                let e = find(bytes, b"endobj", body).unwrap_or(bytes.len());
                (String::from_utf8_lossy(&bytes[body..e]).to_string(), None, e)
            }
        };
        objs.insert(id, Obj { dict, stream });
        at = next;
    }
    Pdf { objs }
}

fn refs_after(dict: &str, key: &str) -> Vec<u32> {
    let Some(i) = dict.find(key) else { return vec![] };
    let rest = &dict[i + key.len()..];
    let end = rest.find(['/', '>']).unwrap_or(rest.len());
    let cleaned = rest[..end].replace(['[', ']'], " ");
    let t: Vec<&str> = cleaned.split_whitespace().collect();
    let mut out = Vec::new();
    for w in t.windows(3) {
        if w[1] == "0" && w[2] == "R" {
            if let Ok(n) = w[0].parse() {
                out.push(n);
            }
        }
    }
    out
}

impl Pdf {
    /// Page objects in page-tree order.
    pub fn pages(&self) -> Vec<u32> {
        let tree = self.objs.values().find(|o| o.dict.contains("/Type /Pages")).expect("a page tree");
        refs_after(&tree.dict, "/Kids")
    }
    pub fn media_box(&self, page: u32) -> [f64; 4] {
        let d = &self.objs[&page].dict;
        let i = d.find("/MediaBox [").unwrap() + 11;
        let v: Vec<f64> = d[i..].split(']').next().unwrap().split_whitespace().map(|t| t.parse().unwrap()).collect();
        [v[0], v[1], v[2], v[3]]
    }
    pub fn content_bytes(&self, page: u32) -> Vec<u8> {
        let c = refs_after(&self.objs[&page].dict, "/Contents")[0];
        self.objs[&c].stream.clone().expect("page content")
    }
    pub fn content(&self, page: u32) -> String {
        String::from_utf8_lossy(&self.content_bytes(page)).to_string()
    }
    /// font resource name (`F0`) -> its glyph-to-character map
    fn cmaps(&self) -> BTreeMap<String, BTreeMap<u32, char>> {
        let mut out = BTreeMap::new();
        for (id, o) in &self.objs {
            if !o.dict.contains("/Subtype /Type0") {
                continue;
            }
            let to = refs_after(&o.dict, "/ToUnicode")[0];
            let text = String::from_utf8_lossy(self.objs[&to].stream.as_ref().unwrap()).to_string();
            let mut map = BTreeMap::new();
            for line in text.lines() {
                let l: Vec<&str> = line.split(['<', '>']).filter(|t| !t.trim().is_empty()).collect();
                if l.len() == 2 && l.iter().all(|t| t.chars().all(|c| c.is_ascii_hexdigit())) {
                    if let (Ok(g), Ok(u)) = (u32::from_str_radix(l[0], 16), u32::from_str_radix(l[1], 16)) {
                        map.insert(g, char::from_u32(u).unwrap_or('?'));
                    }
                }
            }
            // the resource names are F0..F3 for object ids 3..6
            out.insert(format!("F{}", id - 3), map);
        }
        out
    }
    /// The text a page draws, one string per `Tj`.
    pub fn text(&self, page: u32) -> Vec<String> {
        let maps = self.cmaps();
        let b = &self.content_bytes(page)[..];
        let (mut font, mut out, mut i, mut last) = (String::new(), Vec::new(), 0usize, 0usize);
        while i < b.len() {
            match b[i] {
                b'(' => {
                    let start = i;
                    i += 1;
                    let mut depth = 1;
                    while i < b.len() {
                        match b[i] {
                            b'\\' => i += 1,
                            b'(' => depth += 1,
                            b')' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                        i += 1;
                    }
                    let bytes = unescape(&b[start..=i]);
                    assert!(bytes.len() % 2 == 0, "odd string {:?}", String::from_utf8_lossy(&b[start..=i]));
                    let map = &maps[&font];
                    out.push(bytes.chunks(2).map(|c| map.get(&(((c[0] as u32) << 8) | c[1] as u32)).copied().unwrap_or('\u{fffd}')).collect());
                    i += 1;
                    last = i;
                }
                b'T' if b.get(i + 1) == Some(&b'f') => {
                    let before = String::from_utf8_lossy(&b[last..i]).to_string();
                    if let Some(name) = before.split_whitespace().rev().nth(1) {
                        font = name.trim_start_matches('/').to_string();
                    }
                    i += 2;
                    last = i;
                }
                b'\n' => {
                    i += 1;
                    last = i;
                }
                _ => i += 1,
            }
        }
        out
    }
    /// Every distinct stroke width the page uses, in paper millimeters.
    pub fn line_widths_mm(&self, page: u32) -> Vec<f64> {
        let mut out: Vec<f64> = Vec::new();
        for line in self.content(page).lines() {
            if let Some(w) = line.strip_suffix(" w").and_then(|t| t.trim().parse::<f64>().ok()) {
                let mm = w * 25.4 / 72.0;
                if !out.iter().any(|x| (x - mm).abs() < 1e-4) {
                    out.push(mm);
                }
            }
        }
        out.sort_by(|a, b| a.total_cmp(b));
        out
    }
    /// (base font name, has an embedded font file) for every font the file holds.
    pub fn fonts(&self) -> Vec<(String, bool)> {
        self.objs
            .values()
            .filter(|o| o.dict.contains("/Type /FontDescriptor"))
            .map(|o| (o.dict.split("/FontName /").nth(1).unwrap().split_whitespace().next().unwrap().to_string(), o.dict.contains("/FontFile2")))
            .collect()
    }
}

/// A PDF literal string `(...)` to its bytes.
fn unescape(b: &[u8]) -> Vec<u8> {
    let (mut i, mut out) = (1, Vec::new());
    while i + 1 < b.len() {
        if b[i] == b'\\' {
            i += 1;
            if b[i].is_ascii_digit() {
                let mut n = 0u32;
                let mut k = 0;
                while k < 3 && i < b.len() && b[i].is_ascii_digit() {
                    n = n * 8 + (b[i] - b'0') as u32;
                    i += 1;
                    k += 1;
                }
                out.push(n as u8);
                continue;
            }
            out.push(match b[i] {
                b'n' => b'\n',
                b'r' => b'\r',
                b't' => b'\t',
                b'b' => 8,
                b'f' => 12,
                c => c,
            });
        } else {
            out.push(b[i]);
        }
        i += 1;
    }
    out
}
