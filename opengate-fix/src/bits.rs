//! Bit reader for H.264 / HEVC parameter sets.

pub struct BitReader {
    data: Vec<u8>,
    pos: usize, // in bits
}

impl BitReader {
    /// Takes a NAL payload (header already skipped) and strips the
    /// emulation-prevention bytes (00 00 03 -> 00 00).
    pub fn from_nal_payload(payload: &[u8]) -> Self {
        let mut data = Vec::with_capacity(payload.len());
        let mut zeros = 0;
        for &b in payload {
            if zeros >= 2 && b == 3 {
                zeros = 0;
                continue;
            }
            zeros = if b == 0 { zeros + 1 } else { 0 };
            data.push(b);
        }
        Self { data, pos: 0 }
    }

    pub fn bit(&mut self) -> Option<u32> {
        let byte = *self.data.get(self.pos / 8)?;
        let v = (byte >> (7 - (self.pos % 8))) & 1;
        self.pos += 1;
        Some(v as u32)
    }

    pub fn bits(&mut self, n: u32) -> Option<u32> {
        let mut v = 0u32;
        for _ in 0..n {
            v = (v << 1) | self.bit()?;
        }
        Some(v)
    }

    pub fn skip(&mut self, n: usize) -> Option<()> {
        self.pos += n;
        (self.pos <= self.data.len() * 8).then_some(())
    }

    /// Unsigned Exp-Golomb.
    pub fn ue(&mut self) -> Option<u32> {
        let mut zeros = 0;
        while self.bit()? == 0 {
            zeros += 1;
            if zeros > 31 {
                return None;
            }
        }
        Some((1u32 << zeros) - 1 + self.bits(zeros)?)
    }

    /// Signed Exp-Golomb.
    pub fn se(&mut self) -> Option<i32> {
        let k = self.ue()?;
        Some(if k % 2 == 1 { (k / 2 + 1) as i32 } else { -((k / 2) as i32) })
    }
}
