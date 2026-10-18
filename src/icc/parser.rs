//! ICC header + tag-table parsing.

use std::collections::HashMap;

use super::IccError;

pub const HEADER_LEN: usize = 128;

#[derive(Clone, Debug)]
pub(super) struct Tag {
    pub sig: [u8; 4],
    /// Index into `Profile::blobs`; tags that shared data in the source file
    /// keep sharing it.
    pub blob: usize,
}

/// A parsed profile: raw header plus tag payloads, ready to be edited and
/// serialized with freshly computed offsets.
#[derive(Clone, Debug)]
pub struct Profile {
    pub(super) header: [u8; HEADER_LEN],
    pub(super) tags: Vec<Tag>,
    pub(super) blobs: Vec<Vec<u8>>,
}

fn be32(b: &[u8], o: usize) -> usize {
    u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize
}

impl Profile {
    pub fn parse(bytes: &[u8]) -> Result<Profile, IccError> {
        let err = |m: &str| IccError(format!("invalid ICC profile: {m}"));

        if bytes.len() < HEADER_LEN + 4 {
            return Err(err("file too small"));
        }
        let size = be32(bytes, 0);
        if size > bytes.len() {
            return Err(err("declared size exceeds file size (truncated?)"));
        }
        if size < HEADER_LEN + 4 {
            return Err(err("declared size too small"));
        }
        let bytes = &bytes[..size];
        if &bytes[36..40] != b"acsp" {
            return Err(err("missing 'acsp' signature"));
        }
        if &bytes[16..20] != b"RGB " {
            return Err(err(&format!(
                "unsupported colour space '{}' (only RGB profiles are supported)",
                String::from_utf8_lossy(&bytes[16..20])
            )));
        }

        let count = be32(bytes, HEADER_LEN);
        if count > 4096 {
            return Err(err("implausible tag count"));
        }
        let table_end = HEADER_LEN + 4 + 12 * count;
        if table_end > size {
            return Err(err("tag table extends past end of file"));
        }

        let mut header = [0u8; HEADER_LEN];
        header.copy_from_slice(&bytes[..HEADER_LEN]);

        let mut tags: Vec<Tag> = Vec::with_capacity(count);
        let mut blobs: Vec<Vec<u8>> = Vec::new();
        let mut seen: HashMap<(usize, usize), usize> = HashMap::new();

        for i in 0..count {
            let o = HEADER_LEN + 4 + 12 * i;
            let sig = [bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]];
            let off = be32(bytes, o + 4);
            let len = be32(bytes, o + 8);
            let end = off
                .checked_add(len)
                .ok_or_else(|| err("tag offset overflow"))?;
            if off < table_end || end > size {
                return Err(err(&format!(
                    "tag '{}' points outside the file",
                    String::from_utf8_lossy(&sig)
                )));
            }
            if tags.iter().any(|t| t.sig == sig) {
                continue; // keep the first of any duplicated signature
            }
            let blob = *seen.entry((off, len)).or_insert_with(|| {
                blobs.push(bytes[off..end].to_vec());
                blobs.len() - 1
            });
            tags.push(Tag { sig, blob });
        }

        Ok(Profile { header, tags, blobs })
    }

    pub fn tag(&self, sig: &[u8; 4]) -> Option<&[u8]> {
        self.tags
            .iter()
            .find(|t| &t.sig == sig)
            .map(|t| self.blobs[t.blob].as_slice())
    }

    #[allow(dead_code)]
    pub fn tag_count(&self) -> usize {
        self.tags.len()
    }

    /// Replace the tag's data, or add the tag if it does not exist. Other tags
    /// that shared the old data keep it.
    pub fn set_tag(&mut self, sig: [u8; 4], data: Vec<u8>) {
        self.blobs.push(data);
        let blob = self.blobs.len() - 1;
        match self.tags.iter_mut().find(|t| t.sig == sig) {
            Some(t) => t.blob = blob,
            None => self.tags.push(Tag { sig, blob }),
        }
    }
}
