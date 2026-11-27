use super::{invalid, position, supported};
use crate::nbt::{NamedTag, Tag};
use horizon_core::position::{BlockPos, ChunkPos};
use std::io;

/// A restricted, pre-DataVersion Anvil column. Coordinates and section arrays
/// are validated before editing; unrelated legacy tags survive serialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column {
    pub(super) pos: ChunkPos,
    root: NamedTag,
    pub(super) blocks: Vec<u8>,
    pub(super) sky: Vec<u8>,
    pub(super) heights: [i32; 256],
    sections: u16,
}

fn bytes<'a>(tag: &'a Tag, name: &str, len: usize) -> io::Result<&'a [u8]> {
    match tag.get(name) {
        Some(Tag::ByteArray(v)) if v.len() == len => Ok(v),
        _ => Err(invalid(format!("invalid legacy {name} array"))),
    }
}

impl Column {
    pub fn classic(pos: ChunkPos) -> io::Result<Self> {
        position(pos)?;
        let mut blocks = vec![0; 65536];
        blocks[..256].fill(7);
        blocks[256..768].fill(3);
        blocks[768..1024].fill(2);
        let mut sky = vec![15; 65536];
        sky[..1024].fill(0);
        Ok(Self {
            pos,
            root: NamedTag {
                name: Default::default(),
                tag: Tag::compound([(
                    "Level",
                    Tag::compound([
                        ("xPos", Tag::Int(pos.x)),
                        ("zPos", Tag::Int(pos.z)),
                        ("LastUpdate", Tag::Long(0)),
                        ("InhabitedTime", Tag::Long(0)),
                        ("TerrainPopulated", Tag::Byte(1)),
                        ("LightPopulated", Tag::Byte(1)),
                        ("Biomes", Tag::ByteArray(vec![1; 256])),
                        ("Entities", Tag::List(Vec::new())),
                        ("TileEntities", Tag::List(Vec::new())),
                        ("TileTicks", Tag::List(Vec::new())),
                    ]),
                )]),
            },
            blocks,
            sky,
            heights: [4; 256],
            sections: 1,
        })
    }

    pub fn from_nbt(root: NamedTag, pos: ChunkPos) -> io::Result<Self> {
        position(pos)?;
        if root.tag.get("DataVersion").is_some() {
            return Err(invalid("modern chunk in a legacy 1.7 world"));
        }
        let level = root
            .tag
            .get("Level")
            .ok_or_else(|| invalid("missing legacy Level"))?;
        if level.get("DataVersion").is_some()
            || level.get("xPos") != Some(&Tag::Int(pos.x))
            || level.get("zPos") != Some(&Tag::Int(pos.z))
            || bytes(level, "Biomes", 256)?.iter().any(|&v| v != 1)
        {
            return Err(invalid("unsupported legacy column coordinates/biomes"));
        }
        for name in ["Entities", "TileEntities", "TileTicks"] {
            if level
                .get(name)
                .is_some_and(|v| !matches!(v, Tag::List(v) if v.is_empty()))
            {
                return Err(invalid(
                    "entities and scheduled ticks are outside the flat preview",
                ));
            }
        }
        let heights: [i32; 256] = match level.get("HeightMap") {
            Some(Tag::IntArray(v)) if v.len() == 256 && v.iter().all(|h| (0..=256).contains(h)) => {
                v.as_slice().try_into().unwrap()
            }
            _ => return Err(invalid("invalid legacy heightmap")),
        };
        let mut blocks = vec![0; 65536];
        let mut sky = vec![0; 65536];
        for y in 0..256 {
            for cell in 0..256 {
                sky[y * 256 + cell] = if y as i32 >= heights[cell] { 15 } else { 0 };
            }
        }
        let Some(Tag::List(parts)) = level.get("Sections") else {
            return Err(invalid("missing legacy sections"));
        };
        if parts.len() > 16 {
            return Err(invalid("too many legacy sections"));
        }
        let mut sections = 0u16;
        for section in parts {
            let Some(Tag::Byte(y @ 0..=15)) = section.get("Y") else {
                return Err(invalid("invalid legacy section Y"));
            };
            let bit = 1u16 << *y;
            if sections & bit != 0 {
                return Err(invalid("duplicate legacy section"));
            }
            sections |= bit;
            let ids = bytes(section, "Blocks", 4096)?;
            if ids.iter().any(|&id| !supported(u16::from(id), 0))
                || bytes(section, "Data", 2048)?.iter().any(|&v| v != 0)
                || bytes(section, "BlockLight", 2048)?.iter().any(|&v| v != 0)
                || (section.get("Add").is_some()
                    && bytes(section, "Add", 2048)?.iter().any(|&v| v != 0))
            {
                return Err(invalid(
                    "unsupported flat-preview block, metadata or block light",
                ));
            }
            let light = bytes(section, "SkyLight", 2048)?;
            let start = *y as usize * 4096;
            blocks[start..start + 4096].copy_from_slice(ids);
            for i in 0..4096 {
                sky[start + i] = (light[i / 2] >> (4 * (i & 1))) & 15;
            }
        }
        Ok(Self {
            pos,
            root,
            blocks,
            sky,
            heights,
            sections,
        })
    }

    fn index(&self, pos: BlockPos) -> io::Result<usize> {
        if pos.chunk() != self.pos || !(0..256).contains(&pos.y) {
            return Err(invalid("block outside this legacy column"));
        }
        Ok(pos.y as usize * 256 + (pos.z & 15) as usize * 16 + (pos.x & 15) as usize)
    }
    pub fn block(&self, pos: BlockPos) -> io::Result<u8> {
        Ok(self.blocks[self.index(pos)?])
    }
    pub fn skylight(&self, pos: BlockPos) -> io::Result<u8> {
        Ok(self.sky[self.index(pos)?])
    }
    pub fn height(&self, x: u8, z: u8) -> Option<i32> {
        (x < 16 && z < 16).then(|| self.heights[z as usize * 16 + x as usize])
    }
    pub(super) fn set(&mut self, pos: BlockPos, id: u8) -> io::Result<bool> {
        let i = self.index(pos)?;
        let changed = self.blocks[i] != id;
        self.blocks[i] = id;
        if changed {
            self.sections |= 1u16 << (pos.y / 16);
        }
        Ok(changed)
    }
    pub(super) fn update_heights(&mut self) {
        // MC-7508: 1.7's regeneration skips the highest layer of the
        // highest nonempty 16-block section. Retain y=31/255 oracle cases.
        let ceiling = self
            .blocks
            .iter()
            .rposition(|&id| id != 0)
            .map_or(0, |i| (i / 4096 + 1) * 16 - 1);
        for cell in 0..256 {
            self.heights[cell] = (0..ceiling)
                .rev()
                .find(|&y| !matches!(self.blocks[y * 256 + cell], 0 | 20))
                .map_or(0, |y| y as i32 + 1);
        }
    }

    pub fn into_nbt(mut self) -> NamedTag {
        let mut sections = Vec::new();
        for y in 0..16 {
            let start = y * 4096;
            let implicit = (0..4096).all(|i| {
                self.blocks[start + i] == 0
                    && self.sky[start + i]
                        == if (start + i) / 256 >= self.heights[i % 256] as usize {
                            15
                        } else {
                            0
                        }
            });
            if self.sections & (1 << y) == 0 && implicit {
                continue;
            }
            let mut sky = vec![0; 2048];
            for i in 0..4096 {
                sky[i / 2] |= self.sky[start + i] << (4 * (i & 1));
            }
            sections.push(Tag::compound([
                ("Y", Tag::Byte(y as i8)),
                (
                    "Blocks",
                    Tag::ByteArray(self.blocks[start..start + 4096].to_vec()),
                ),
                ("Data", Tag::ByteArray(vec![0; 2048])),
                ("BlockLight", Tag::ByteArray(vec![0; 2048])),
                ("SkyLight", Tag::ByteArray(sky)),
            ]));
        }
        let Tag::Compound(level) = self.root.tag.get_mut("Level").unwrap() else {
            unreachable!()
        };
        level.insert("Sections".into(), Tag::List(sections));
        level.insert("HeightMap".into(), Tag::IntArray(self.heights.to_vec()));
        self.root
    }
}
