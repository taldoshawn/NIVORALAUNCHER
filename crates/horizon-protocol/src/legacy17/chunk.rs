//! Protocol 4/5 columns: zlib-compressed block IDs, metadata, light, Add and biomes.
//! The 1.7 wire format is decoded before adapting numeric states for the renderer.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    frame::{MAX_FRAME, MAX_UNCOMPRESSED},
};
use flate2::{Decompress, FlushDecompress, Status};

const BLOCKS: usize = 4096;
const NIBBLES: usize = 2048;

#[derive(Debug)]
pub struct Column {
    pub x: i32,
    pub z: i32,
    pub ground_up: bool,
    pub mask: u16,
    pub add_mask: u16,
    pub skylight: bool,
    unload: bool,
    data: Vec<u8>,
}

#[derive(Debug)]
pub struct Section<'a> {
    pub y: u8,
    blocks: &'a [u8],
    metadata: &'a [u8],
    add: Option<&'a [u8]>,
    block_light: &'a [u8],
    sky_light: Option<&'a [u8]>,
}

fn expected_len(mask: u16, add: u16, ground_up: bool, skylight: bool) -> Result<usize> {
    if add & !mask != 0 {
        return Err(Error::new("1.7 Add mask contains an absent section"));
    }
    Ok(
        mask.count_ones() as usize * (BLOCKS + NIBBLES * (2 + usize::from(skylight)))
            + add.count_ones() as usize * NIBBLES
            + if ground_up { 256 } else { 0 },
    )
}

fn compressed_length(r: &mut Reader<'_>) -> Result<usize> {
    let length = usize::try_from(r.i32()?)
        .map_err(|_| Error::new("negative 1.7 compressed chunk length"))?;
    if length > MAX_FRAME {
        return Err(Error::new("1.7 compressed chunk exceeds frame limit"));
    }
    Ok(length)
}

/// One complete zlib stream, bounded before allocation and checked through its
/// checksum. A second stream, trailing garbage, truncation and bombs are errors.
fn inflate(bytes: &[u8], limit: usize) -> Result<Vec<u8>> {
    if limit > MAX_UNCOMPRESSED {
        return Err(Error::new("1.7 chunk inflation exceeds memory limit"));
    }
    let mut decoded = vec![0; limit + 1];
    let mut zlib = Decompress::new(true);
    let status = zlib
        .decompress(bytes, &mut decoded, FlushDecompress::Finish)
        .map_err(|e| Error::new(format!("invalid 1.7 chunk zlib stream: {e}")))?;
    if status != Status::StreamEnd
        || zlib.total_in() != bytes.len() as u64
        || zlib.total_out() > limit as u64
    {
        return Err(Error::new(
            "incomplete, oversized or trailing 1.7 chunk data",
        ));
    }
    decoded.truncate(zlib.total_out() as usize);
    Ok(decoded)
}

/// Packet 0x26 has the compressed body before its column headers, unlike 1.8.
/// Its skylight flag describes every column; all columns include biome bytes.
/// The shared inflation budget is the transport's existing 8 MiB quota.
pub fn decode_bulk(payload: &[u8]) -> Result<Vec<Column>> {
    let mut r = Reader::new(payload);
    let count =
        usize::try_from(r.i16()?).map_err(|_| Error::new("negative 1.7 bulk chunk count"))?;
    let length = compressed_length(&mut r)?;
    let skylight = r.bool()?;
    let compressed = r.bytes(length)?;
    if r.remaining() != count * 12 {
        return Err(Error::new(
            "1.7 bulk chunk header count does not match payload",
        ));
    }
    let mut headers = Vec::with_capacity(count);
    let mut expected = 0usize;
    for _ in 0..count {
        let (x, z, mask, add_mask) = (r.i32()?, r.i32()?, r.u16()?, r.u16()?);
        let size = expected_len(mask, add_mask, true, skylight)?;
        expected += size;
        if expected > MAX_UNCOMPRESSED {
            return Err(Error::new("1.7 bulk chunks exceed inflation budget"));
        }
        headers.push((x, z, mask, add_mask, size));
    }
    let decoded = if expected == 0 && compressed.is_empty() {
        Vec::new()
    } else {
        inflate(compressed, expected)?
    };
    if decoded.len() != expected {
        return Err(Error::new("1.7 bulk chunk length does not match its masks"));
    }
    let mut offset = 0;
    Ok(headers
        .into_iter()
        .map(|(x, z, mask, add_mask, size)| {
            let data = decoded[offset..offset + size].to_vec();
            offset += size;
            Column {
                x,
                z,
                ground_up: true,
                mask,
                add_mask,
                skylight,
                unload: false,
                data,
            }
        })
        .collect())
}

impl Column {
    /// Decode packet 0x21 without its packet ID. The current dimension determines
    /// skylight; guessing from inflated size would accept malformed columns.
    pub fn decode(payload: &[u8], skylight: bool) -> Result<Self> {
        let mut r = Reader::new(payload);
        let (x, z, ground_up, mask, add_mask) = (r.i32()?, r.i32()?, r.bool()?, r.u16()?, r.u16()?);
        let expected = expected_len(mask, add_mask, ground_up, skylight)?;
        let length = compressed_length(&mut r)?;
        let compressed = r.bytes(length)?;
        if r.remaining() != 0 {
            return Err(Error::new("trailing bytes in 1.7 chunk packet"));
        }
        let unload = ground_up && mask == 0;
        // Vanilla may compress an empty/biome-only unload; some compatible
        // servers send a zero compressed length instead.
        let data = if unload && compressed.is_empty() {
            Vec::new()
        } else {
            inflate(compressed, expected)?
        };
        if data.len() != expected && !(unload && data.is_empty()) {
            return Err(Error::new("1.7 chunk length does not match its masks"));
        }
        Ok(Self {
            x,
            z,
            ground_up,
            mask,
            add_mask,
            skylight,
            unload,
            data,
        })
    }

    pub fn is_unload(&self) -> bool {
        self.unload
    }

    pub fn biomes(&self) -> Option<&[u8]> {
        (self.ground_up && !self.unload).then(|| &self.data[self.data.len() - 256..])
    }

    pub fn section(&self, y: u8) -> Option<Section<'_>> {
        if y >= 16 || self.mask & (1u16 << y) == 0 {
            return None;
        }
        let preceding = (1u16 << y) - 1;
        let index = (self.mask & preceding).count_ones() as usize;
        let count = self.mask.count_ones() as usize;
        let metadata = count * BLOCKS + index * NIBBLES;
        let block_light = count * (BLOCKS + NIBBLES) + index * NIBBLES;
        let sky_light = count * (BLOCKS + 2 * NIBBLES) + index * NIBBLES;
        let add = count * (BLOCKS + NIBBLES * (2 + usize::from(self.skylight)))
            + (self.add_mask & preceding).count_ones() as usize * NIBBLES;
        Some(Section {
            y,
            blocks: &self.data[index * BLOCKS..(index + 1) * BLOCKS],
            metadata: &self.data[metadata..metadata + NIBBLES],
            add: (self.add_mask & (1u16 << y) != 0).then(|| &self.data[add..add + NIBBLES]),
            block_light: &self.data[block_light..block_light + NIBBLES],
            sky_light: self
                .skylight
                .then(|| &self.data[sky_light..sky_light + NIBBLES]),
        })
    }

    /// Produce an uncompressed protocol-47-shaped payload for the existing
    /// numeric-state renderer, not for sending over a 1.7 connection. No ID is
    /// included. An empty bulk column becomes one explicit air section so a
    /// single-column decoder cannot mistake it for an unload.
    pub fn to_protocol47_payload(&self) -> Vec<u8> {
        let empty_bulk = self.ground_up && self.mask == 0 && !self.unload;
        let mask = if empty_bulk { 1 } else { self.mask };
        let mut data = Vec::new();
        if !self.unload {
            if empty_bulk {
                data.resize(8192 + NIBBLES, 0);
                if self.skylight {
                    data.resize(data.len() + NIBBLES, 0xff);
                }
            } else {
                for section in (0..16).filter_map(|y| self.section(y)) {
                    for state in section.states() {
                        data.extend_from_slice(&state.to_le_bytes());
                    }
                }
                for section in (0..16).filter_map(|y| self.section(y)) {
                    data.extend_from_slice(section.block_light);
                }
                if self.skylight {
                    for section in (0..16).filter_map(|y| self.section(y)) {
                        data.extend_from_slice(section.sky_light.unwrap());
                    }
                }
            }
            if let Some(biomes) = self.biomes() {
                data.extend_from_slice(biomes);
            }
        }
        let mut out = Writer::new();
        out.i32(self.x);
        out.i32(self.z);
        out.bool(self.ground_up);
        out.u16(mask);
        out.byte_array(&data);
        out.data
    }
}

fn nibble(bytes: &[u8], index: usize) -> u8 {
    (bytes[index / 2] >> ((index & 1) * 4)) & 15
}

impl Section<'_> {
    fn index(x: u8, y: u8, z: u8) -> Option<usize> {
        (x < 16 && y < 16 && z < 16)
            .then_some((usize::from(y) * 16 + usize::from(z)) * 16 + usize::from(x))
    }

    fn state_at(&self, index: usize) -> u16 {
        (u16::from(self.add.map_or(0, |add| nibble(add, index))) << 12)
            | (u16::from(self.blocks[index]) << 4)
            | u16::from(nibble(self.metadata, index))
    }

    pub fn state(&self, x: u8, y: u8, z: u8) -> Option<u16> {
        Self::index(x, y, z).map(|index| self.state_at(index))
    }

    pub fn states(&self) -> impl Iterator<Item = u16> + '_ {
        (0..BLOCKS).map(|index| self.state_at(index))
    }

    pub fn block_light(&self, x: u8, y: u8, z: u8) -> Option<u8> {
        Self::index(x, y, z).map(|index| nibble(self.block_light, index))
    }

    pub fn sky_light(&self, x: u8, y: u8, z: u8) -> Option<u8> {
        let index = Self::index(x, y, z)?;
        self.sky_light.map(|sky| nibble(sky, index))
    }

    pub fn packed_light(&self) -> impl Iterator<Item = u8> + '_ {
        (0..BLOCKS).map(|index| {
            nibble(self.block_light, index)
                | (self.sky_light.map_or(0, |sky| nibble(sky, index)) << 4)
        })
    }

    pub fn uniform_light(&self) -> Option<u8> {
        let mut values = self.packed_light();
        let first = values.next()?;
        values.all(|light| light == first).then_some(first)
    }
}
