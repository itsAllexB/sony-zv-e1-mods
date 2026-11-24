//! Reads the real picture size out of the codec configuration (hvcC / avcC).
//! This is the size the decoder will actually produce, whatever the MP4
//! container claims.

use crate::bits::BitReader;

/// Display size from an `hvcC` box payload (HEVC).
pub fn hevc_size_from_hvcc(hvcc: &[u8]) -> Option<(u32, u32)> {
    let mut p = 22;
    let arrays = *hvcc.get(p)?;
    p += 1;
    for _ in 0..arrays {
        let nal_type = hvcc.get(p)? & 0x3f;
        let count = u16::from_be_bytes([*hvcc.get(p + 1)?, *hvcc.get(p + 2)?]);
        p += 3;
        for _ in 0..count {
            let len = u16::from_be_bytes([*hvcc.get(p)?, *hvcc.get(p + 1)?]) as usize;
            p += 2;
            let nal = hvcc.get(p..p + len)?;
            p += len;
            if nal_type == 33 {
                return hevc_sps_size(nal);
            }
        }
    }
    None
}

fn hevc_sps_size(nal: &[u8]) -> Option<(u32, u32)> {
    let mut r = BitReader::from_nal_payload(nal.get(2..)?);
    r.skip(4)?; // sps_video_parameter_set_id
    let max_sub_layers = r.bits(3)?;
    r.skip(1)?; // temporal_id_nesting
    // profile_tier_level: general part is 88 bits + 8 bits level
    r.skip(96)?;
    let mut present = Vec::new();
    for _ in 0..max_sub_layers {
        present.push((r.bit()?, r.bit()?));
    }
    if max_sub_layers > 0 {
        r.skip(2 * (8 - max_sub_layers as usize))?;
    }
    for (profile, level) in present {
        if profile == 1 {
            r.skip(88)?;
        }
        if level == 1 {
            r.skip(8)?;
        }
    }
    r.ue()?; // sps_seq_parameter_set_id
    let chroma = r.ue()?;
    let mut separate = 0;
    if chroma == 3 {
        separate = r.bit()?;
    }
    let width = r.ue()?;
    let height = r.ue()?;
    let (mut l, mut rgt, mut t, mut b) = (0, 0, 0, 0);
    if r.bit()? == 1 {
        l = r.ue()?;
        rgt = r.ue()?;
        t = r.ue()?;
        b = r.ue()?;
    }
    let (sub_w, sub_h) = match (chroma, separate) {
        (1, _) => (2, 2),
        (2, _) => (2, 1),
        _ => (1, 1),
    };
    let w = width.checked_sub(sub_w * (l.checked_add(rgt)?))?;
    let h = height.checked_sub(sub_h * (t.checked_add(b)?))?;
    (w > 0 && h > 0).then_some((w, h))
}

/// Display size from an `avcC` box payload (H.264).
pub fn avc_size_from_avcc(avcc: &[u8]) -> Option<(u32, u32)> {
    let count = avcc.get(5)? & 0x1f;
    if count == 0 {
        return None;
    }
    let len = u16::from_be_bytes([*avcc.get(6)?, *avcc.get(7)?]) as usize;
    avc_sps_size(avcc.get(8..8 + len)?)
}

fn avc_sps_size(nal: &[u8]) -> Option<(u32, u32)> {
    let mut r = BitReader::from_nal_payload(nal.get(1..)?);
    let profile = r.bits(8)?;
    r.skip(16)?; // constraint flags + level
    r.ue()?; // seq_parameter_set_id
    let mut chroma = 1;
    let mut separate = 0;
    if matches!(profile, 100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134 | 135) {
        chroma = r.ue()?;
        if chroma == 3 {
            separate = r.bit()?;
        }
        r.ue()?; // bit_depth_luma_minus8
        r.ue()?; // bit_depth_chroma_minus8
        r.skip(1)?; // qpprime_y_zero_transform_bypass
        if r.bit()? == 1 {
            let lists = if chroma != 3 { 8 } else { 12 };
            for i in 0..lists {
                if r.bit()? == 1 {
                    let size = if i < 6 { 16 } else { 64 };
                    let (mut last, mut next) = (8i32, 8i32);
                    for _ in 0..size {
                        if next != 0 {
                            next = (last + r.se()? + 256) % 256;
                        }
                        if next != 0 {
                            last = next;
                        }
                    }
                }
            }
        }
    }
    r.ue()?; // log2_max_frame_num_minus4
    match r.ue()? {
        0 => {
            r.ue()?;
        }
        1 => {
            r.skip(1)?;
            r.se()?;
            r.se()?;
            let n = r.ue()?;
            if n > 255 {
                return None;
            }
            for _ in 0..n {
                r.se()?;
            }
        }
        _ => {}
    }
    r.ue()?; // max_num_ref_frames
    r.skip(1)?; // gaps_in_frame_num_value_allowed
    let w_mbs = r.ue()?.checked_add(1)?;
    let h_units = r.ue()?.checked_add(1)?;
    let frame_mbs_only = r.bit()?;
    if frame_mbs_only == 0 {
        r.skip(1)?;
    }
    r.skip(1)?; // direct_8x8_inference
    let (mut l, mut rgt, mut t, mut b) = (0, 0, 0, 0);
    if r.bit()? == 1 {
        l = r.ue()?;
        rgt = r.ue()?;
        t = r.ue()?;
        b = r.ue()?;
    }
    let (sub_w, sub_h) = match (chroma, separate) {
        (1, _) => (2, 2),
        (2, _) => (2, 1),
        _ => (1, 1),
    };
    let field = 2 - frame_mbs_only;
    let w = w_mbs.checked_mul(16)?.checked_sub(sub_w * (l.checked_add(rgt)?))?;
    let h = (field * h_units).checked_mul(16)?.checked_sub(sub_h * field * (t.checked_add(b)?))?;
    (w > 0 && h > 0).then_some((w, h))
}
