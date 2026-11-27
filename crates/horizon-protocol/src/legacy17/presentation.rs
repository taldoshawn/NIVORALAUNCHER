//! Presentation packets shared by protocols 4 and 5, without a modern wire codec.
//!
//! Wire facts were checked against Cuberite's Apache-2.0 1.7EOL reference
//! (72aa50f710200832f97a0022dc70c5cd531b6aea) and the MIT MCProtocolLib map
//! reader (d0c110f204e24232e0a3db03c776f16d33692361). This is an original
//! implementation; the live 1.7 server fixture checks the observed formats.
use crate::{Error, Result, buf::Reader};

#[derive(Clone, Debug, PartialEq)]
pub enum Packet {
    Collect {
        item: i32,
        collector: i32,
    },
    Attach {
        entity: i32,
        vehicle: i32,
        leash: bool,
    },
    Sound {
        name: String,
        position: [i32; 3],
        volume: f32,
        pitch: f32,
    },
    Sign {
        position: [i32; 3],
        lines: [String; 4],
    },
    Map {
        id: i32,
        update: MapUpdate,
    },
    OpenSign {
        position: [i32; 3],
    },
    Plugin {
        channel: String,
        data: Vec<u8>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MapUpdate {
    /// One vertical slice of the 128 by 128 map, starting at (x, y).
    Column {
        x: u8,
        y: u8,
        colors: Vec<u8>,
    },
    /// Replaces the current decorations; an empty list clears them.
    Icons(Vec<MapIcon>),
    Scale(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapIcon {
    pub kind: u8,
    pub rotation: u8,
    pub x: i8,
    pub y: i8,
}

impl Packet {
    /// None means another packet family owns this ID. An owned packet must
    /// consume exactly its frame, including its nested short-sized payload.
    pub fn decode(id: i32, bytes: &[u8]) -> Result<Option<Self>> {
        let mut r = Reader::new(bytes);
        let packet = match id {
            0x0d => {
                let item = entity(&mut r)?;
                let collector = entity(&mut r)?;
                Self::Collect { item, collector }
            }
            0x1b => {
                let entity = entity(&mut r)?;
                let vehicle = r.i32()?;
                if vehicle < -1 {
                    return Err(Error::new("invalid protocol 4/5 attachment target"));
                }
                let leash = match r.u8()? {
                    0 => false,
                    1 => true,
                    _ => return Err(Error::new("invalid protocol 4/5 attachment flag")),
                };
                Self::Attach {
                    entity,
                    vehicle,
                    leash,
                }
            }
            0x29 => {
                let name = r.string(256)?;
                let position = [r.i32()?, r.i32()?, r.i32()?];
                let volume = r.f32()?;
                // Pitch is unsigned, including values at and above 128.
                let pitch = f32::from(r.u8()?) / 63.0;
                if name.is_empty() || !volume.is_finite() || volume < 0.0 {
                    return Err(Error::new("invalid protocol 4/5 named sound"));
                }
                Self::Sound {
                    name,
                    position,
                    volume,
                    pitch,
                }
            }
            0x33 => {
                let position = [r.i32()?, i32::from(r.i16()?), r.i32()?];
                check_sign_position(position)?;
                // These are four plain strings, not JSON chat components.
                let lines = [r.string(15)?, r.string(15)?, r.string(15)?, r.string(15)?];
                Self::Sign { position, lines }
            }
            0x34 => {
                let id = r.varint()?;
                if id < 0 {
                    return Err(Error::new("negative protocol 4/5 map ID"));
                }
                let update = map_update(short_payload(&mut r)?)?;
                Self::Map { id, update }
            }
            0x36 => {
                // The editor uses int Y, whereas sign text uses short Y.
                let position = [r.i32()?, r.i32()?, r.i32()?];
                check_sign_position(position)?;
                Self::OpenSign { position }
            }
            0x3f => {
                let channel = r.string(20)?;
                if channel.is_empty() {
                    return Err(Error::new("empty protocol 4/5 plugin channel"));
                }
                let data = short_payload(&mut r)?.to_vec();
                Self::Plugin { channel, data }
            }
            _ => return Ok(None),
        };
        super::finish(&r)?;
        Ok(Some(packet))
    }
}

fn entity(r: &mut Reader<'_>) -> Result<i32> {
    let id = r.i32()?;
    if id < 0 {
        return Err(Error::new("negative protocol 4/5 entity ID"));
    }
    Ok(id)
}

fn short_payload<'a>(r: &mut Reader<'a>) -> Result<&'a [u8]> {
    let length = usize::try_from(r.i16()?)
        .map_err(|_| Error::new("negative protocol 4/5 payload length"))?;
    r.bytes(length)
}

pub(super) fn check_sign_position(position: [i32; 3]) -> Result<()> {
    if !(0..256).contains(&position[1]) {
        return Err(Error::new("protocol 4/5 sign Y is outside the world"));
    }
    Ok(())
}

fn map_update(bytes: &[u8]) -> Result<MapUpdate> {
    let mut r = Reader::new(bytes);
    let update = match r.u8()? {
        0 => {
            let x = r.u8()?;
            let y = r.u8()?;
            let height = r.remaining();
            if x >= 128 || y >= 128 || height == 0 || height > 128 - usize::from(y) {
                return Err(Error::new("invalid protocol 4/5 map column"));
            }
            MapUpdate::Column {
                x,
                y,
                colors: r.rest().to_vec(),
            }
        }
        1 => {
            if !r.remaining().is_multiple_of(3) {
                return Err(Error::new("incomplete protocol 4/5 map decoration"));
            }
            let mut icons = Vec::with_capacity(r.remaining() / 3);
            while r.remaining() != 0 {
                let packed = r.u8()?;
                icons.push(MapIcon {
                    // MCProtocolLib's 1.7 reader and real vanilla agree:
                    // type in the high nibble, rotation in the low nibble.
                    kind: packed >> 4,
                    rotation: packed & 15,
                    x: r.i8()?,
                    y: r.i8()?,
                });
            }
            MapUpdate::Icons(icons)
        }
        2 => {
            let scale = r.u8()?;
            if scale > 4 {
                return Err(Error::new("invalid protocol 4/5 map scale"));
            }
            MapUpdate::Scale(scale)
        }
        _ => return Err(Error::new("unknown protocol 4/5 map update")),
    };
    super::finish(&r)?;
    Ok(update)
}

/// 1.7's MC|Brand payload is raw UTF-8, without an inner string length.
/// Other plugin channels retain their bytes and have no implied semantics.
pub fn server_brand(channel: &str, data: &[u8]) -> Result<Option<String>> {
    if channel != "MC|Brand" {
        return Ok(None);
    }
    let brand = std::str::from_utf8(data)
        .map_err(|_| Error::new("invalid protocol 4/5 server brand UTF-8"))?;
    Ok(Some(brand.to_owned()))
}

pub fn map_icon_name(kind: u8) -> Option<&'static str> {
    Some(match kind {
        0 => "minecraft:player",
        1 => "minecraft:frame",
        2 => "minecraft:red_marker",
        3 => "minecraft:blue_marker",
        4 => "minecraft:target_x",
        5 => "minecraft:target_point",
        6 => "minecraft:player_off_map",
        7 => "minecraft:player_off_limits",
        _ => return None,
    })
}

/// The deliberately limited aliases checked against Cuberite's Apache-2.0
/// sound migration dae2adde6563bec4b614230992cae6b1ca965b4f. Unknown names
/// are not guessed; a caller may still resolve an exact resource-pack name.
pub fn sound_name(name: &str) -> Option<&'static str> {
    Some(match name.strip_prefix("minecraft:").unwrap_or(name) {
        "random.bowhit" => "minecraft:entity.arrow.hit",
        "random.bow" => "minecraft:entity.arrow.shoot",
        "random.successful_hit" => "minecraft:entity.arrow.hit_player",
        "random.pop" => "minecraft:entity.item.pickup",
        "random.orb" => "minecraft:entity.experience_orb.pickup",
        "random.break" => "minecraft:entity.item.break",
        "random.chestopen" => "minecraft:block.chest.open",
        "random.chestclosed" => "minecraft:block.chest.close",
        "random.explode" => "minecraft:entity.generic.explode",
        "mob.zombie.hurt" => "minecraft:entity.zombie.hurt",
        "mob.zombie.death" => "minecraft:entity.zombie.death",
        _ => return None,
    })
}
