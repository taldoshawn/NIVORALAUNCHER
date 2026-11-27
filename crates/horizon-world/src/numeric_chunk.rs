//! Native storage for pre-flattening Anvil chunks (1.7.10, 1.8.9 and 1.12.2).
//! Keep the original NBT tree so entities, lighting and unrelated tags survive edits.
use crate::{
    compression::invalid,
    nbt::{NamedTag, Tag},
};
use horizon_core::position::{BlockPos, ChunkPos};
use std::io;

pub struct NumericChunk {
    root: NamedTag,
    position: ChunkPos,
}

fn array<'a>(tag: &'a Tag, name: &str, size: usize) -> io::Result<&'a [u8]> {
    match tag.get(name) {
        Some(Tag::ByteArray(bytes)) if bytes.len() == size => Ok(bytes),
        _ => Err(invalid(format!("invalid numeric section {name} array"))),
    }
}

fn nibble(bytes: &[u8], i: usize) -> u8 {
    (bytes[i / 2] >> ((i % 2) * 4)) & 15
}

impl NumericChunk {
    /// Generate the terrain stage of the classic superflat preset, before structures
    /// or population. Legacy skylight uses one nibble per block.
    pub fn classic_flat_189(position: ChunkPos) -> Self {
        let mut blocks = vec![0; 4096];
        blocks[..256].fill(7);
        blocks[256..768].fill(3);
        blocks[768..1024].fill(2);
        let mut sky = vec![0xff; 2048];
        sky[..512].fill(0);
        let section = Tag::compound([
            ("Y", Tag::Byte(0)),
            ("Blocks", Tag::ByteArray(blocks)),
            ("Data", Tag::ByteArray(vec![0; 2048])),
            ("BlockLight", Tag::ByteArray(vec![0; 2048])),
            ("SkyLight", Tag::ByteArray(sky)),
        ]);
        let level = Tag::compound([
            ("V", Tag::Byte(1)),
            ("xPos", Tag::Int(position.x)),
            ("zPos", Tag::Int(position.z)),
            ("Sections", Tag::List(vec![section])),
            ("Biomes", Tag::ByteArray(vec![1; 256])),
            ("HeightMap", Tag::IntArray(vec![4; 256])),
            ("TerrainPopulated", Tag::Byte(0)),
            ("LightPopulated", Tag::Byte(0)),
            ("InhabitedTime", Tag::Long(0)),
            ("LastUpdate", Tag::Long(0)),
            ("Entities", Tag::List(vec![])),
            ("TileEntities", Tag::List(vec![])),
        ]);
        Self {
            position,
            root: NamedTag {
                name: Default::default(),
                tag: Tag::compound([("Level", level)]),
            },
        }
    }

    pub fn new(root: NamedTag) -> io::Result<Self> {
        if let Some(version) = root.tag.get("DataVersion")
            && !matches!(version, Tag::Int(0..=1343))
        {
            return Err(invalid("unsupported numeric Anvil data version"));
        }
        let level = root
            .tag
            .get("Level")
            .ok_or_else(|| invalid("missing Level"))?;
        let (Some(Tag::Int(x)), Some(Tag::Int(z)), Some(Tag::List(sections))) =
            (level.get("xPos"), level.get("zPos"), level.get("Sections"))
        else {
            return Err(invalid("missing numeric chunk coordinates or Sections"));
        };
        let position = ChunkPos::new(*x, *z);
        let mut seen = 0u16;
        for section in sections {
            let Some(Tag::Byte(y @ 0..=15)) = section.get("Y") else {
                return Err(invalid("invalid numeric section Y"));
            };
            let bit = 1u16 << y;
            if seen & bit != 0 {
                return Err(invalid("duplicate numeric section"));
            }
            seen |= bit;
            array(section, "Blocks", 4096)?;
            array(section, "Data", 2048)?;
            if section.get("Add").is_some() {
                array(section, "Add", 2048)?;
            }
        }
        Ok(Self { root, position })
    }

    fn section_index(&self, pos: BlockPos) -> io::Result<Option<usize>> {
        if pos.chunk() != self.position || !(0..256).contains(&pos.y) {
            return Err(invalid("block outside numeric chunk"));
        }
        let Some(Tag::List(sections)) = self.root.tag.get("Level").and_then(|l| l.get("Sections"))
        else {
            unreachable!()
        };
        Ok(sections
            .iter()
            .position(|s| s.get("Y") == Some(&Tag::Byte((pos.y >> 4) as i8))))
    }

    /// Returns (numeric block ID, metadata). Absent sections contain air.
    pub fn block(&self, pos: BlockPos) -> io::Result<(u16, u8)> {
        let Some(index) = self.section_index(pos)? else {
            return Ok((0, 0));
        };
        let Some(Tag::List(sections)) = self.root.tag.get("Level").and_then(|l| l.get("Sections"))
        else {
            unreachable!()
        };
        let section = &sections[index];
        let i = pos.section_index();
        let high = match section.get("Add") {
            Some(Tag::ByteArray(bytes)) => nibble(bytes, i),
            _ => 0,
        };
        Ok((
            (u16::from(high) << 8) | u16::from(array(section, "Blocks", 4096)?[i]),
            nibble(array(section, "Data", 2048)?, i),
        ))
    }

    /// Update raw block data only. Lighting and heightmaps belong to the simulator.
    pub fn set_block(&mut self, pos: BlockPos, id: u16, metadata: u8) -> io::Result<()> {
        if id > 4095 || metadata > 15 {
            return Err(invalid("numeric block ID or metadata out of range"));
        }
        let index = self.section_index(pos)?;
        let Some(Tag::List(sections)) = self
            .root
            .tag
            .get_mut("Level")
            .and_then(|l| l.get_mut("Sections"))
        else {
            unreachable!()
        };
        let section = match index {
            Some(index) => &mut sections[index],
            None if id == 0 && metadata == 0 => return Ok(()),
            None => {
                sections.push(Tag::compound([
                    ("Y", Tag::Byte((pos.y >> 4) as i8)),
                    ("Blocks", Tag::ByteArray(vec![0; 4096])),
                    ("Data", Tag::ByteArray(vec![0; 2048])),
                    ("BlockLight", Tag::ByteArray(vec![0; 2048])),
                    ("SkyLight", Tag::ByteArray(vec![0; 2048])),
                ]));
                sections.last_mut().unwrap()
            }
        };
        let i = pos.section_index();
        let Some(Tag::ByteArray(blocks)) = section.get_mut("Blocks") else {
            unreachable!()
        };
        blocks[i] = id as u8;
        if id > 255 && section.get("Add").is_none() {
            let Tag::Compound(tags) = section else {
                unreachable!()
            };
            tags.insert("Add".into(), Tag::ByteArray(vec![0; 2048]));
        }
        for (key, value) in [("Data", metadata), ("Add", (id >> 8) as u8)] {
            if let Some(Tag::ByteArray(bytes)) = section.get_mut(key) {
                let shift = (i % 2) * 4;
                bytes[i / 2] = (bytes[i / 2] & !(15 << shift)) | (value << shift);
            }
        }
        Ok(())
    }

    pub fn into_nbt(self) -> NamedTag {
        self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{compression::Compression, nbt::Limits, region::ChunkRecord};
    #[test]
    fn classic_flat_layers_survive_compressed_anvil_record() {
        let chunk = NumericChunk::classic_flat_189(ChunkPos::new(-2, 3));
        let record =
            ChunkRecord::from_nbt(&chunk.into_nbt(), Compression::Zlib, 0, Limits::default())
                .unwrap();
        let loaded = NumericChunk::new(record.nbt(Limits::default()).unwrap()).unwrap();
        for x in -32..-16 {
            for z in 48..64 {
                for (y, id) in [
                    (0, 7),
                    (1, 3),
                    (2, 3),
                    (3, 2),
                    (4, 0),
                    (15, 0),
                    (16, 0),
                    (255, 0),
                ] {
                    assert_eq!(loaded.block(BlockPos::new(x, y, z)).unwrap(), (id, 0));
                }
            }
        }
    }
    #[test]
    fn sparse_negative_chunk_round_trip_preserves_neighbour_nibbles_and_custom_tags() {
        let root = NamedTag {
            name: "".into(),
            tag: Tag::compound([(
                "Level",
                Tag::compound([
                    ("xPos", Tag::Int(-1)),
                    ("zPos", Tag::Int(0)),
                    ("Sections", Tag::List(vec![])),
                    ("extra", Tag::Long(123)),
                ]),
            )]),
        };
        let mut chunk = NumericChunk::new(root).unwrap();
        let even = BlockPos::new(-2, 255, 4);
        let odd = even.offset(1, 0, 0);
        assert_eq!(chunk.block(even).unwrap(), (0, 0));
        chunk.set_block(even, 0xf12, 7).unwrap();
        chunk.set_block(odd, 0x834, 11).unwrap();
        chunk.set_block(even, 3, 2).unwrap();
        assert!(chunk.set_block(odd, 4096, 0).is_err());
        assert!(chunk.block(odd.offset(1, 0, 0)).is_err());
        let record =
            ChunkRecord::from_nbt(&chunk.into_nbt(), Compression::Zlib, 0, Limits::default())
                .unwrap();
        let loaded = NumericChunk::new(record.nbt(Limits::default()).unwrap()).unwrap();
        assert_eq!(loaded.block(even).unwrap(), (3, 2));
        assert_eq!(loaded.block(odd).unwrap(), (0x834, 11));
        assert_eq!(
            loaded.into_nbt().tag.get("Level").unwrap().get("extra"),
            Some(&Tag::Long(123))
        );
    }
}
