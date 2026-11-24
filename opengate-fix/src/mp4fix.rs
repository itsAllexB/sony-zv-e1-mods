//! Finds every place in a Sony MP4 that states the frame size and works out
//! the byte patches that make them agree with the video stream itself.
//!
//! Open gate clips from a modded camera carry a 3840x2560 HEVC stream, but the
//! camera still writes the stock 3840x2160 into four header fields:
//!
//! 1. `moov/trak/tkhd`                     (track display size)
//! 2. `moov/trak/.../stsd/hvc1`            (sample entry size)
//! 3. `uuid PROF` -> `VPRF`                (Sony profile box)
//! 4. `meta/xml ` NonRealTimeMeta          (embedded copy of the sidecar XML)
//!
//! Nothing else is touched: video, audio and the `rtmd` metadata track (gyro,
//! lens, IBIS data) stay byte-identical, and no box changes size.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::sps;

const MAX_MOOV: u64 = 512 * 1024 * 1024;
const MAX_XML: u64 = 16 * 1024 * 1024;
const PROF_UUID: [u8; 16] = [
    0x50, 0x52, 0x4f, 0x46, 0x21, 0xd2, 0x4f, 0xce, 0xbb, 0x88, 0x69, 0x5c, 0xfa, 0xc9, 0xc7, 0x40,
];

#[derive(Debug, Clone)]
pub struct Patch {
    pub offset: u64,
    pub old: Vec<u8>,
    pub new: Vec<u8>,
    pub what: &'static str,
}

#[derive(Debug, Clone)]
pub struct Analysis {
    /// Size the MP4 header claims.
    pub container: (u32, u32),
    /// Size the video stream really has.
    pub stream: (u32, u32),
    pub patches: Vec<Patch>,
}

impl Analysis {
    pub fn needs_fix(&self) -> bool {
        self.container != self.stream
    }
}

struct BoxRef {
    kind: [u8; 4],
    /// Offset of the payload (after the size/type header).
    start: u64,
    /// End of the box.
    end: u64,
}

fn be32(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

fn be16(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

/// Lists the boxes found in `file[start..end]`.
fn file_boxes(f: &mut File, start: u64, end: u64) -> std::io::Result<Vec<BoxRef>> {
    let mut out = Vec::new();
    let mut pos = start;
    while pos + 8 <= end {
        f.seek(SeekFrom::Start(pos))?;
        let mut h = [0u8; 16];
        f.read_exact(&mut h[..8])?;
        let mut size = u32::from_be_bytes(h[..4].try_into().unwrap()) as u64;
        let mut header = 8;
        if size == 1 {
            f.read_exact(&mut h[8..16])?;
            size = u64::from_be_bytes(h[8..16].try_into().unwrap());
            header = 16;
        } else if size == 0 {
            size = end - pos;
        }
        if size < header || pos + size > end {
            break;
        }
        out.push(BoxRef { kind: h[4..8].try_into().unwrap(), start: pos + header, end: pos + size });
        pos += size;
    }
    Ok(out)
}

/// Lists the boxes found in `data[start..end]` (offsets relative to `data`).
fn mem_boxes(data: &[u8], start: usize, end: usize) -> Vec<BoxRef> {
    let mut out = Vec::new();
    let mut pos = start;
    while pos + 8 <= end {
        let mut size = be32(data, pos).unwrap() as u64;
        let mut header = 8u64;
        if size == 1 {
            if pos + 16 > end {
                break;
            }
            size = u64::from_be_bytes(data[pos + 8..pos + 16].try_into().unwrap());
            header = 16;
        } else if size == 0 {
            size = (end - pos) as u64;
        }
        if size < header || pos as u64 + size > end as u64 {
            break;
        }
        out.push(BoxRef {
            kind: data[pos + 4..pos + 8].try_into().unwrap(),
            start: pos as u64 + header,
            end: pos as u64 + size,
        });
        pos += size as usize;
    }
    out
}

fn child<'a>(boxes: &'a [BoxRef], kind: &[u8; 4]) -> Option<&'a BoxRef> {
    boxes.iter().find(|b| &b.kind == kind)
}

fn read_range(f: &mut File, start: u64, end: u64) -> std::io::Result<Vec<u8>> {
    let mut buf = vec![0u8; (end - start) as usize];
    f.seek(SeekFrom::Start(start))?;
    f.read_exact(&mut buf)?;
    Ok(buf)
}

pub fn analyze(path: &Path) -> Result<Analysis, String> {
    let mut f = File::open(path).map_err(|e| format!("Can't open file: {e}"))?;
    let len = f.metadata().map_err(|e| e.to_string())?.len();
    let top = file_boxes(&mut f, 0, len).map_err(|e| format!("Can't read file: {e}"))?;
    if child(&top, b"ftyp").is_none() {
        return Err("Not an MP4/MOV video".into());
    }
    let moov_ref = child(&top, b"moov").ok_or("No index found (recording may be incomplete)")?;
    if moov_ref.end - moov_ref.start > MAX_MOOV {
        return Err("Unsupported file (index too large)".into());
    }
    let moov_at = moov_ref.start;
    let moov = read_range(&mut f, moov_ref.start, moov_ref.end).map_err(|e| e.to_string())?;

    // First video track.
    let mut found = None;
    for trak in mem_boxes(&moov, 0, moov.len()).iter().filter(|b| &b.kind == b"trak") {
        let tb = mem_boxes(&moov, trak.start as usize, trak.end as usize);
        let (Some(tkhd), Some(mdia)) = (child(&tb, b"tkhd"), child(&tb, b"mdia")) else { continue };
        let mb = mem_boxes(&moov, mdia.start as usize, mdia.end as usize);
        let Some(hdlr) = child(&mb, b"hdlr") else { continue };
        if moov.get(hdlr.start as usize + 8..hdlr.start as usize + 12) != Some(b"vide") {
            continue;
        }
        let Some(minf) = child(&mb, b"minf") else { continue };
        let minf_b = mem_boxes(&moov, minf.start as usize, minf.end as usize);
        let Some(stbl) = child(&minf_b, b"stbl") else { continue };
        let stbl_b = mem_boxes(&moov, stbl.start as usize, stbl.end as usize);
        let Some(stsd) = child(&stbl_b, b"stsd") else { continue };
        found = Some((tkhd.start as usize, stsd.start as usize, stsd.end as usize));
        break;
    }
    let (tkhd_at, stsd_at, stsd_end) = found.ok_or("No video track found")?;

    // Sample entry: the size the container claims, plus the codec config.
    let entry = stsd_at + 8;
    let entry_size = be32(&moov, entry).ok_or("Damaged video header")? as usize;
    let entry_end = (entry + entry_size).min(stsd_end);
    let fourcc: [u8; 4] = moov.get(entry + 4..entry + 8).ok_or("Damaged video header")?.try_into().unwrap();
    let codec = String::from_utf8_lossy(&fourcc).to_string();
    let dims_at = entry + 32;
    let cw = be16(&moov, dims_at).ok_or("Damaged video header")? as u32;
    let ch = be16(&moov, dims_at + 2).ok_or("Damaged video header")? as u32;

    let config = mem_boxes(&moov, entry + 86, entry_end);
    let stream = match &fourcc {
        b"hvc1" | b"hev1" => child(&config, b"hvcC")
            .and_then(|b| sps::hevc_size_from_hvcc(&moov[b.start as usize..b.end as usize])),
        b"avc1" | b"avc3" => child(&config, b"avcC")
            .and_then(|b| sps::avc_size_from_avcc(&moov[b.start as usize..b.end as usize])),
        _ => return Err(format!("Unsupported video codec ({})", codec.trim())),
    }
    .ok_or("Can't read the video stream's frame size")?;
    let (sw, sh) = stream;

    let mut a = Analysis { container: (cw, ch), stream, patches: Vec::new() };
    if !a.needs_fix() {
        return Ok(a);
    }
    if sw > u16::MAX as u32 || sh > u16::MAX as u32 {
        return Err("Unsupported frame size".into());
    }
    let dims16 = |w: u32, h: u32| [(w as u16).to_be_bytes(), (h as u16).to_be_bytes()].concat();

    // 2. sample entry
    a.patches.push(Patch {
        offset: moov_at + dims_at as u64,
        old: dims16(cw, ch),
        new: dims16(sw, sh),
        what: "sample entry",
    });

    // 1. tkhd (16.16 fixed point). Only when it mirrors the sample entry.
    let version = *moov.get(tkhd_at).ok_or("Damaged track header")?;
    let tk_dims = tkhd_at + 4 + if version == 1 { 32 } else { 20 } + 16 + 36;
    let fixed = |w: u32, h: u32| [(w << 16).to_be_bytes(), (h << 16).to_be_bytes()].concat();
    if moov.get(tk_dims..tk_dims + 8) == Some(&fixed(cw, ch)[..]) {
        a.patches.push(Patch {
            offset: moov_at + tk_dims as u64,
            old: fixed(cw, ch),
            new: fixed(sw, sh),
            what: "track header",
        });
    }

    // 3. Sony PROF/VPRF
    for b in top.iter().filter(|b| &b.kind == b"uuid" && b.end - b.start >= 24 && b.end - b.start < 65536) {
        let Ok(data) = read_range(&mut f, b.start, b.end) else { continue };
        if data[..16] != PROF_UUID {
            continue;
        }
        for sub in mem_boxes(&data, 24, data.len()).iter().filter(|s| &s.kind == b"VPRF") {
            // Offsets are from the start of the VPRF box: width at +40, height at +42.
            let at = sub.start as usize - 8 + 40;
            if data.get(at..at + 4) == Some(&dims16(cw, ch)[..]) {
                a.patches.push(Patch {
                    offset: b.start + at as u64,
                    old: dims16(cw, ch),
                    new: dims16(sw, sh),
                    what: "Sony profile box",
                });
            }
        }
    }

    // 4. embedded NonRealTimeMeta XML
    if let Some(meta) = child(&top, b"meta") {
        if let Ok(kids) = file_boxes(&mut f, meta.start + 4, meta.end) {
            if let Some(x) = child(&kids, b"xml ").filter(|x| x.end - x.start > 4 && x.end - x.start < MAX_XML) {
                if let Ok(old) = read_range(&mut f, x.start + 4, x.end) {
                    if let Some(new) = fix_xml(&old, a.container, a.stream, true) {
                        if new != old {
                            a.patches.push(Patch { offset: x.start + 4, old, new, what: "embedded XML" });
                        }
                    }
                }
            }
        }
    }
    Ok(a)
}

/// Writes the patches, refusing if the bytes on disk aren't what was analysed.
pub fn apply(f: &mut File, patches: &[Patch]) -> Result<(), String> {
    for p in patches {
        let mut cur = vec![0u8; p.old.len()];
        f.seek(SeekFrom::Start(p.offset)).map_err(|e| e.to_string())?;
        f.read_exact(&mut cur).map_err(|e| e.to_string())?;
        if cur != p.old || p.old.len() != p.new.len() {
            return Err(format!("Unexpected data in {} — file left untouched", p.what));
        }
    }
    for p in patches {
        f.seek(SeekFrom::Start(p.offset)).map_err(|e| e.to_string())?;
        f.write_all(&p.new).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn ratio(w: u32, h: u32) -> String {
    let g = gcd(w, h).max(1);
    format!("{}:{}", w / g, h / g)
}

/// Rewrites the frame size inside Sony's NonRealTimeMeta XML.
/// With `keep_len` the result has exactly the input's byte length (needed when
/// patching the copy embedded in the MP4); returns None if that can't be done.
pub fn fix_xml(xml: &[u8], from: (u32, u32), to: (u32, u32), keep_len: bool) -> Option<Vec<u8>> {
    let mut s = std::str::from_utf8(xml).ok()?.to_string();
    let original_len = s.len();

    // videoCodec="HEVC_3840_2160_M10P@L5HT"
    if let Some(i) = s.find("videoCodec=\"") {
        let v0 = i + 12;
        if let Some(n) = s[v0..].find('"') {
            let value = s[v0..v0 + n].replace(&format!("_{}_{}_", from.0, from.1), &format!("_{}_{}_", to.0, to.1));
            s.replace_range(v0..v0 + n, &value);
        }
    }
    let mut set = |attr: &str, old: String, new: String| {
        let from = format!("{attr}=\"{old}\"");
        if let Some(i) = s.find(&from) {
            s.replace_range(i..i + from.len(), &format!("{attr}=\"{new}\""));
        }
    };
    set("pixel", from.0.to_string(), to.0.to_string());
    set("numOfVerticalLine", from.1.to_string(), to.1.to_string());
    set("aspectRatio", ratio(from.0, from.1), ratio(to.0, to.1));

    if keep_len {
        if s.len() > original_len {
            return None;
        }
        if s.len() < original_len {
            // Pad with spaces just before the end of the VideoLayout tag.
            let layout = s.find("<VideoLayout")?;
            let close = layout + s[layout..].find("/>")?;
            s.insert_str(close, &" ".repeat(original_len - s.len()));
        }
    }
    Some(s.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::fix_xml;

    const XML: &str = r#"<VideoFrame videoCodec="HEVC_3840_2160_M10P@L5HT" captureFps="23.98p"/>
<VideoLayout pixel="3840" numOfVerticalLine="2160" aspectRatio="16:9"/>"#;

    #[test]
    fn xml_is_rewritten() {
        let out = String::from_utf8(fix_xml(XML.as_bytes(), (3840, 2160), (3840, 2560), false).unwrap()).unwrap();
        assert!(out.contains("HEVC_3840_2560_M10P@L5HT"));
        assert!(out.contains(r#"numOfVerticalLine="2560" aspectRatio="3:2"/>"#));
    }

    #[test]
    fn embedded_xml_keeps_its_length() {
        let out = fix_xml(XML.as_bytes(), (3840, 2160), (3840, 2560), true).unwrap();
        assert_eq!(out.len(), XML.len());
        assert!(String::from_utf8(out).unwrap().contains(r#"aspectRatio="3:2" />"#));
    }
}
