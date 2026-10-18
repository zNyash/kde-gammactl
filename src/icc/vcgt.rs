//! The `vcgt` (video card gamma table) tag, table variant.
//!
//! Layout (all big-endian):
//! `'vcgt'` | 0u32 | gammaType u32 (0 = table) | channels u16 | entries u16 |
//! entrySize u16 | channels * entries * entrySize bytes.

use super::IccError;
use crate::curves::Curve;

pub const SIGNATURE: [u8; 4] = *b"vcgt";

pub fn encode(curve: &Curve) -> Result<Vec<u8>, IccError> {
    let n = curve.red.len();
    if n < 2 || n > u16::MAX as usize || curve.green.len() != n || curve.blue.len() != n {
        return Err(IccError(format!("cannot encode a {n}-entry vcgt table")));
    }
    let mut out = Vec::with_capacity(18 + 3 * n * 2);
    out.extend_from_slice(&SIGNATURE);
    out.extend_from_slice(&0u32.to_be_bytes()); // reserved
    out.extend_from_slice(&0u32.to_be_bytes()); // gammaType: table
    out.extend_from_slice(&3u16.to_be_bytes()); // channels
    out.extend_from_slice(&(n as u16).to_be_bytes());
    out.extend_from_slice(&2u16.to_be_bytes()); // 16-bit entries
    for ch in [&curve.red, &curve.green, &curve.blue] {
        for v in ch.iter() {
            out.extend_from_slice(&v.to_be_bytes());
        }
    }
    Ok(out)
}

pub fn decode(data: &[u8]) -> Result<Curve, IccError> {
    let err = |m: &str| IccError(format!("unsupported or malformed vcgt tag: {m}"));
    if data.len() < 18 || data[..4] != SIGNATURE {
        return Err(err("bad header"));
    }
    let kind = u32::from_be_bytes(data[8..12].try_into().unwrap());
    if kind != 0 {
        return Err(err("formula-based vcgt is not supported"));
    }
    let channels = u16::from_be_bytes(data[12..14].try_into().unwrap()) as usize;
    let n = u16::from_be_bytes(data[14..16].try_into().unwrap()) as usize;
    let size = u16::from_be_bytes(data[16..18].try_into().unwrap()) as usize;
    if channels != 3 || n == 0 || !(size == 1 || size == 2) {
        return Err(err("expected 3 channels with 8- or 16-bit entries"));
    }
    if data.len() < 18 + channels * n * size {
        return Err(err("table is truncated"));
    }
    let body = &data[18..];
    let read = |ch: usize| -> Vec<u16> {
        (0..n)
            .map(|i| {
                let o = (ch * n + i) * size;
                if size == 2 {
                    u16::from_be_bytes([body[o], body[o + 1]])
                } else {
                    u16::from(body[o]) * 257
                }
            })
            .collect()
    };
    Ok(Curve {
        red: read(0),
        green: read(1),
        blue: read(2),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curves::{generate, Adjustments};

    #[test]
    fn roundtrip() {
        let c = generate(&Adjustments::new(0.7, 1.1, 1.3).unwrap(), 256).unwrap();
        let bytes = encode(&c).unwrap();
        assert_eq!(bytes.len(), 18 + 3 * 256 * 2);
        assert_eq!(decode(&bytes).unwrap(), c);
    }

    #[test]
    fn decodes_eight_bit_tables() {
        let mut data = Vec::new();
        data.extend_from_slice(b"vcgt");
        data.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]);
        data.extend_from_slice(&3u16.to_be_bytes());
        data.extend_from_slice(&2u16.to_be_bytes());
        data.extend_from_slice(&1u16.to_be_bytes());
        data.extend_from_slice(&[0, 255, 0, 255, 0, 255]);
        let c = decode(&data).unwrap();
        assert_eq!(c.red, vec![0, 65535]);
    }

    #[test]
    fn rejects_formula_type() {
        let mut data = vec![0u8; 18];
        data[..4].copy_from_slice(b"vcgt");
        data[8..12].copy_from_slice(&1u32.to_be_bytes());
        assert!(decode(&data).is_err());
    }
}
