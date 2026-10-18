//! Minimal ICC profile tag editor (Phase 2 + 4).
//!
//! Only what kde-gammactl needs: parse a profile's header and tag table, add or
//! replace the `vcgt` tag, and write a structurally valid profile back with
//! rebuilt offsets and sizes. Every other tag is carried over byte-for-byte.

mod parser;
pub mod vcgt;
mod writer;

use std::fmt;

use crate::curves::Curve;
pub use parser::Profile;

#[derive(Debug)]
pub struct IccError(pub String);

impl fmt::Display for IccError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for IccError {}

/// Build the profile bytes for `curve`.
///
/// `baseline` is the untouched original profile; `None` synthesizes a neutral
/// sRGB display profile instead. The baseline is never modified: a new byte
/// buffer is returned, so adjustments cannot compound.
pub fn build_profile(baseline: Option<&[u8]>, curve: &Curve) -> Result<Vec<u8>, IccError> {
    let mut profile = match baseline {
        Some(bytes) => Profile::parse(bytes)?,
        None => Profile::default_srgb(),
    };
    profile.set_tag(vcgt::SIGNATURE, vcgt::encode(curve)?);
    let out = profile.to_bytes();

    // Self-check: what we wrote must parse and round-trip its curve.
    let check = Profile::parse(&out)?;
    let tag = check
        .tag(&vcgt::SIGNATURE)
        .ok_or_else(|| IccError("generated profile lost its vcgt tag".into()))?;
    if &vcgt::decode(tag)? != curve {
        return Err(IccError(
            "generated vcgt does not match the requested curve".into(),
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curves::{generate, Adjustments, CurveKind};

    fn curve() -> Curve {
        generate(
            CurveKind::Classic,
            &Adjustments::new(0.8, 0.9, 0.7).unwrap(),
            256,
        )
        .unwrap()
    }

    #[test]
    fn synthesized_profile_is_valid_and_has_vcgt() {
        let bytes = build_profile(None, &curve()).unwrap();
        let p = Profile::parse(&bytes).unwrap();
        assert!(p.tag(b"wtpt").is_some());
        assert!(p.tag(b"rXYZ").is_some());
        assert!(p.tag(b"rTRC").is_some());
        assert_eq!(
            vcgt::decode(p.tag(&vcgt::SIGNATURE).unwrap()).unwrap(),
            curve()
        );
        // size field matches, 4-byte aligned
        assert_eq!(
            u32::from_be_bytes(bytes[0..4].try_into().unwrap()) as usize,
            bytes.len()
        );
        assert_eq!(bytes.len() % 4, 0);
    }

    #[test]
    fn editing_preserves_other_tags_and_replaces_vcgt() {
        let first = build_profile(None, &curve()).unwrap();
        let other = generate(
            CurveKind::Classic,
            &Adjustments::new(1.0, 1.0, 1.0).unwrap(),
            256,
        )
        .unwrap();
        // Use the first output as a baseline that already has a vcgt.
        let second = build_profile(Some(&first), &other).unwrap();

        let a = Profile::parse(&first).unwrap();
        let b = Profile::parse(&second).unwrap();
        for sig in [
            b"desc", b"cprt", b"wtpt", b"rXYZ", b"gXYZ", b"bXYZ", b"rTRC", b"gTRC", b"bTRC",
        ] {
            assert_eq!(
                a.tag(sig),
                b.tag(sig),
                "tag {:?} changed",
                std::str::from_utf8(sig)
            );
        }
        assert_eq!(vcgt::decode(b.tag(b"vcgt").unwrap()).unwrap(), other);
        // exactly one vcgt entry
        assert_eq!(b.tag_count(), a.tag_count());
    }

    #[test]
    fn baseline_without_vcgt_gets_one_added() {
        let mut p = Profile::default_srgb();
        assert!(p.tag(b"vcgt").is_none());
        let bytes = p.to_bytes();
        let out = build_profile(Some(&bytes), &curve()).unwrap();
        let q = Profile::parse(&out).unwrap();
        assert_eq!(q.tag_count(), p.tag_count() + 1);
        p.set_tag(*b"vcgt", vec![0; 4]);
    }

    #[test]
    fn rejects_garbage() {
        assert!(Profile::parse(&[0u8; 10]).is_err());
        let mut bytes = build_profile(None, &curve()).unwrap();
        bytes[36] = b'x'; // break 'acsp'
        assert!(Profile::parse(&bytes).is_err());
        let good = build_profile(None, &curve()).unwrap();
        assert!(Profile::parse(&good[..good.len() - 8]).is_err()); // truncated
    }

    #[test]
    fn rejects_non_rgb_profiles() {
        let mut bytes = build_profile(None, &curve()).unwrap();
        bytes[16..20].copy_from_slice(b"GRAY");
        assert!(Profile::parse(&bytes).is_err());
    }
}
