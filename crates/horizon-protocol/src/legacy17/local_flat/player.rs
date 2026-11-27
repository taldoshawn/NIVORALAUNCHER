use super::{HOTBAR, Version};
use crate::{
    Error, Result,
    crypto::{offline_uuid, uuid_string},
    legacy17::play::Pose,
};
use horizon_world::{
    legacy17_flat::World,
    nbt::{self, Limits, NamedTag, Tag},
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const LIMIT: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Player {
    pub pose: Pose,
    pub held: u8,
    pub flying: bool,
    pub on_ground: bool,
}
impl Default for Player {
    fn default() -> Self {
        Self {
            pose: Pose {
                x: 0.5,
                y: 4.,
                z: 0.5,
                yaw: 0.,
                pitch: 0.,
            },
            held: 0,
            flying: false,
            on_ground: true,
        }
    }
}
fn path(directory: &Path, version: Version, name: &str) -> PathBuf {
    match version {
        Version::V1_7_2 => directory.join("players").join(format!("{name}.dat")),
        Version::V1_7_10 => directory
            .join("playerdata")
            .join(format!("{}.dat", uuid_string(offline_uuid(name)))),
    }
}
fn limits() -> Limits {
    Limits {
        max_bytes: LIMIT,
        max_nodes: 4096,
        max_depth: 32,
    }
}
fn inventory() -> Tag {
    Tag::List(
        HOTBAR
            .iter()
            .enumerate()
            .map(|(index, &id)| {
                Tag::compound([
                    ("Slot", Tag::Byte(index as i8)),
                    ("id", Tag::Short(i16::from(id))),
                    ("Count", Tag::Byte(64)),
                    ("Damage", Tag::Short(0)),
                ])
            })
            .collect(),
    )
}
impl Player {
    pub fn validate(&self) -> Result<()> {
        self.pose.validate()?;
        if self.pose.x.abs() >= 29_999_900.
            || self.pose.z.abs() >= 29_999_900.
            || !(-64.0..=4096.0).contains(&self.pose.y)
            || self.pose.pitch.abs() > 90.
            || self.held > 8
        {
            return Err(Error::new("invalid restricted 1.7 player state"));
        }
        Ok(())
    }
    pub fn load(directory: &Path, version: Version, name: &str) -> Result<Self> {
        let mut file = match File::open(path(directory, version, name)) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(error.into()),
        };
        let mut compressed = Vec::new();
        (&mut file)
            .take(LIMIT as u64 + 1)
            .read_to_end(&mut compressed)?;
        if compressed.len() > LIMIT {
            return Err(Error::new("oversized 1.7 player save"));
        }
        let mut decoder = flate2::bufread::GzDecoder::new(compressed.as_slice());
        let mut bytes = Vec::new();
        (&mut decoder)
            .take(LIMIT as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > LIMIT || !decoder.get_ref().is_empty() {
            return Err(Error::new("oversized or trailing 1.7 player gzip"));
        }
        let (root, used) =
            nbt::read_named_legacy(&bytes, limits()).map_err(|e| Error::new(e.to_string()))?;
        let tag = &root.tag;
        let uuid = offline_uuid(name);
        if used != bytes.len()
            || tag.get("NivoraFlatSchema") != Some(&Tag::Int(1))
            || tag.get("NivoraProtocol") != Some(&Tag::Int(version.protocol()))
            || tag.get("NivoraProfile") != Some(&Tag::String(name.into()))
            || tag.get("UUIDMost") != Some(&Tag::Long((uuid >> 64) as i64))
            || tag.get("UUIDLeast") != Some(&Tag::Long(uuid as i64))
            || tag.get("Dimension") != Some(&Tag::Int(0))
            || tag.get("playerGameType") != Some(&Tag::Int(1))
            || tag.get("Inventory") != Some(&inventory())
        {
            return Err(Error::new("foreign or unsupported 1.7 preview player save"));
        }
        let Some(Tag::List(pos)) = tag.get("Pos") else {
            return Err(Error::new("missing player position"));
        };
        let [Tag::Double(x), Tag::Double(y), Tag::Double(z)] = pos.as_slice() else {
            return Err(Error::new("invalid player position"));
        };
        let Some(Tag::List(rotation)) = tag.get("Rotation") else {
            return Err(Error::new("missing player rotation"));
        };
        let [Tag::Float(yaw), Tag::Float(pitch)] = rotation.as_slice() else {
            return Err(Error::new("invalid player rotation"));
        };
        let Some(Tag::Int(held @ 0..=8)) = tag.get("SelectedItemSlot") else {
            return Err(Error::new("invalid saved hotbar selection"));
        };
        let Some(Tag::Byte(ground @ 0..=1)) = tag.get("OnGround") else {
            return Err(Error::new("invalid saved ground flag"));
        };
        let abilities = tag
            .get("abilities")
            .ok_or_else(|| Error::new("missing saved abilities"))?;
        let Some(Tag::Byte(flying @ 0..=1)) = abilities.get("flying") else {
            return Err(Error::new("invalid saved flight flag"));
        };
        for name in ["invulnerable", "mayfly", "instabuild"] {
            if abilities.get(name) != Some(&Tag::Byte(1)) {
                return Err(Error::new("invalid restricted creative abilities"));
            }
        }
        let player = Self {
            pose: Pose {
                x: f64::from_bits(*x),
                y: f64::from_bits(*y),
                z: f64::from_bits(*z),
                yaw: f32::from_bits(*yaw),
                pitch: f32::from_bits(*pitch),
            },
            held: *held as u8,
            flying: *flying != 0,
            on_ground: *ground != 0,
        };
        player.validate()?;
        Ok(player)
    }
    fn nbt(self, version: Version, name: &str) -> NamedTag {
        let uuid = offline_uuid(name);
        NamedTag {
            name: Default::default(),
            tag: Tag::compound([
                ("NivoraFlatSchema", Tag::Int(1)),
                ("NivoraProtocol", Tag::Int(version.protocol())),
                ("NivoraProfile", Tag::String(name.into())),
                ("UUIDMost", Tag::Long((uuid >> 64) as i64)),
                ("UUIDLeast", Tag::Long(uuid as i64)),
                (
                    "Pos",
                    Tag::List(vec![
                        Tag::Double(self.pose.x.to_bits()),
                        Tag::Double(self.pose.y.to_bits()),
                        Tag::Double(self.pose.z.to_bits()),
                    ]),
                ),
                ("Motion", Tag::List(vec![Tag::Double(0); 3])),
                (
                    "Rotation",
                    Tag::List(vec![
                        Tag::Float(self.pose.yaw.to_bits()),
                        Tag::Float(self.pose.pitch.to_bits()),
                    ]),
                ),
                ("OnGround", Tag::Byte(i8::from(self.on_ground))),
                ("Dimension", Tag::Int(0)),
                ("playerGameType", Tag::Int(1)),
                ("FallDistance", Tag::Float(0)),
                ("Health", Tag::Float(20f32.to_bits())),
                ("foodLevel", Tag::Int(20)),
                ("foodSaturationLevel", Tag::Float(5f32.to_bits())),
                ("SelectedItemSlot", Tag::Int(i32::from(self.held))),
                ("Inventory", inventory()),
                (
                    "abilities",
                    Tag::compound([
                        ("flying", Tag::Byte(i8::from(self.flying))),
                        ("mayfly", Tag::Byte(1)),
                        ("instabuild", Tag::Byte(1)),
                        ("invulnerable", Tag::Byte(1)),
                        ("flySpeed", Tag::Float(0.05f32.to_bits())),
                        ("walkSpeed", Tag::Float(0.1f32.to_bits())),
                    ]),
                ),
            ]),
        }
    }
    pub fn save(self, world: &World, directory: &Path, version: Version, name: &str) -> Result<()> {
        self.validate()?;
        world.check_session()?;
        // Revalidate any existing file before replacing it; foreign/corrupt
        // files are retained, even if introduced during this session.
        Self::load(directory, version, name)?;
        let bytes = nbt::write_named_legacy(&self.nbt(version, name), limits())
            .map_err(|e| Error::new(e.to_string()))?;
        let path = path(directory, version, name);
        fs::create_dir_all(path.parent().unwrap())?;
        let temporary = path.with_extension("dat.tmp");
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let mut gzip = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        gzip.write_all(&bytes)?;
        let file = gzip.finish()?;
        file.sync_all()?;
        drop(file);
        world.check_session()?;
        fs::rename(temporary, path)?;
        Ok(())
    }
}
