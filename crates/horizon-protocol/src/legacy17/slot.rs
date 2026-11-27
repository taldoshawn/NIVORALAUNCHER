//! Protocol 4/5 slots use a signed-short length and gzip-compressed named NBT.
//! The shared slot value has no wire encoding attached to these free functions.
pub use crate::legacy18::slot::Slot;
use crate::{
    Error, Result,
    buf::{NBT_QUOTA, Reader, Writer},
};
use flate2::{Compression, bufread::GzDecoder, write::GzEncoder};
use horizon_world::nbt::{self, Tag};
use std::io::{Read, Write};

fn limits() -> nbt::Limits {
    nbt::Limits {
        max_bytes: NBT_QUOTA,
        ..Default::default()
    }
}

/// Tags added after 1.7 and mixed-list wrappers must not be silently transmitted
/// to a client that only understands the original homogeneous NBT lists.
fn validate_tag(tag: &Tag) -> Result<()> {
    if !matches!(tag, Tag::End | Tag::Compound(_)) {
        return Err(Error::new("1.7 item NBT root must be a compound"));
    }
    let mut pending = vec![tag];
    while let Some(tag) = pending.pop() {
        match tag {
            Tag::LongArray(_) => return Err(Error::new("1.7 NBT has no long-array tag")),
            Tag::Compound(values) => pending.extend(values.values()),
            Tag::List(values) => {
                if let Some(first) = values.first()
                    && values.iter().any(|tag| tag.id() != first.id())
                {
                    return Err(Error::new("1.7 NBT lists must be homogeneous"));
                }
                pending.extend(values);
            }
            _ => {}
        }
    }
    Ok(())
}

pub fn read(r: &mut Reader<'_>) -> Result<Option<Slot>> {
    let id = r.i16()?;
    if id == -1 {
        return Ok(None);
    }
    if id < 0 {
        return Err(Error::new("invalid 1.7 item ID"));
    }
    let count = r.i8()?;
    let damage = r.i16()?;
    let length = r.i16()?;
    let tag = if length == -1 {
        Tag::End
    } else {
        let length =
            usize::try_from(length).map_err(|_| Error::new("invalid 1.7 item NBT length"))?;
        let compressed = r.bytes(length)?;
        // BufRead keeps trailing bytes visible, unlike a buffering Read decoder.
        let mut gzip = GzDecoder::new(compressed);
        let mut bytes = Vec::new();
        gzip.by_ref()
            .take(NBT_QUOTA as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > NBT_QUOTA || !gzip.get_ref().is_empty() {
            return Err(Error::new("oversized or trailing 1.7 item gzip data"));
        }
        let (root, used) = nbt::read_named_legacy(&bytes, limits())
            .map_err(|e| Error::new(format!("1.7 item NBT: {e}")))?;
        if used != bytes.len() || !matches!(root.tag, Tag::Compound(_)) {
            return Err(Error::new("invalid or trailing 1.7 item NBT"));
        }
        validate_tag(&root.tag)?;
        root.tag
    };
    Ok((count > 0).then_some(Slot {
        id,
        count,
        damage,
        tag,
    }))
}

/// Validate and encode before touching the writer, so a failed item cannot leave
/// a partial packet that accidentally gets transmitted by the caller.
pub fn write(slot: Option<&Slot>, w: &mut Writer) -> Result<()> {
    let Some(slot) = slot else {
        w.i16(-1);
        return Ok(());
    };
    if slot.id < 0 || slot.count <= 0 {
        return Err(Error::new("invalid 1.7 slot"));
    }
    validate_tag(&slot.tag)?;
    let compressed = if slot.tag == Tag::End {
        None
    } else {
        let root = nbt::NamedTag {
            name: "".into(),
            tag: slot.tag.clone(),
        };
        let bytes = nbt::write_named_legacy(&root, limits())
            .map_err(|e| Error::new(format!("1.7 item NBT: {e}")))?;
        let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
        gzip.write_all(&bytes)?;
        let compressed = gzip.finish()?;
        if compressed.len() > i16::MAX as usize {
            return Err(Error::new("compressed 1.7 item NBT exceeds short length"));
        }
        Some(compressed)
    };
    w.i16(slot.id);
    w.i8(slot.count);
    w.i16(slot.damage);
    if let Some(compressed) = compressed {
        w.i16(compressed.len() as i16);
        w.bytes(&compressed);
    } else {
        w.i16(-1);
    }
    Ok(())
}
