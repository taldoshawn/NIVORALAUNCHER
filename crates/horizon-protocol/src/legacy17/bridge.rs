//! Protocol 4/5 adapters for the existing numeric-block renderer and action model.
//!
//! Protocol 47 is only an internal representation here. Every outbound action is
//! re-encoded before it reaches the 1.7 socket; unknown inbound packets are never
//! passed through as if their layouts were compatible.
use super::{Version, chunk, slot};
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    legacy18,
};
use std::collections::HashMap;

pub type RenderPacket = (i32, Vec<u8>);

pub struct Incoming {
    version: Version,
    skylight: bool,
    players: HashMap<String, u128>,
}

impl Incoming {
    pub fn new(version: Version) -> Self {
        Self {
            version,
            skylight: true,
            players: HashMap::new(),
        }
    }

    /// Returns packets understood by the shared renderer, including an explicit
    /// player-info update before a player spawn. Unimplemented packet kinds are
    /// ignored, rather than interpreted using a different version's layout.
    pub fn translate(&mut self, id: i32, bytes: &[u8]) -> Result<Vec<RenderPacket>> {
        if bytes.len() > crate::frame::MAX_UNCOMPRESSED {
            return Err(Error::new("oversized 1.7 packet"));
        }
        let mut r = Reader::new(bytes);
        let mut w = Writer::new();
        match id {
            0x00 => w.varint(r.i32()?),
            0x01 => {
                w.i32(r.i32()?);
                w.u8(r.u8()?);
                let dimension = r.i8()?;
                self.skylight = dimension == 0;
                w.i8(dimension);
                w.u8(r.u8()?);
                w.u8(r.u8()?);
                w.string(&r.string(16)?);
                w.bool(false);
            }
            0x02 => {
                w.string(&r.string(32767)?);
                w.u8(0);
            }
            0x03 => {
                w.i64(r.i64()?);
                w.i64(r.i64()?);
            }
            0x04 => {
                w.varint(r.i32()?);
                w.i16(r.i16()?);
                upgrade_slot(&mut r, &mut w)?;
            }
            0x06 => {
                w.f32(r.f32()?);
                let food = r.i16()?;
                if food < 0 {
                    return Err(Error::new("negative 1.7 food level"));
                }
                w.varint(i32::from(food));
                w.f32(r.f32()?);
            }
            0x07 => {
                let dimension = r.i32()?;
                self.skylight = dimension == 0;
                w.i32(dimension);
                w.u8(r.u8()?);
                w.u8(r.u8()?);
                w.string(&r.string(16)?);
            }
            0x08 => {
                let x = r.f64()?;
                let y = r.f64()? - f64::from(1.62_f32);
                let z = r.f64()?;
                let yaw = r.f32()?;
                let pitch = r.f32()?;
                r.bool()?;
                if ![x, y, z, f64::from(yaw), f64::from(pitch)]
                    .iter()
                    .all(|v| v.is_finite())
                {
                    return Err(Error::new("non-finite 1.7 teleport"));
                }
                w.f64(x);
                w.f64(y);
                w.f64(z);
                w.f32(yaw);
                w.f32(pitch);
                w.u8(0); // All axes are absolute in 1.7.
            }
            0x09 => w.u8(r.u8()?),
            0x0b => {
                w.varint(r.varint()?);
                w.u8(r.u8()?);
            }
            0x0c => return self.player_spawn(&mut r),
            0x0e => {
                // Object spawns already use VarInt entity IDs in 1.7.
                w.varint(r.varint()?);
                w.u8(r.u8()?);
                w.bytes(r.bytes(14)?); // fixed-point XYZ, pitch and yaw
                let data = r.i32()?;
                w.i32(data);
                if data != 0 {
                    w.bytes(r.bytes(6)?);
                }
            }
            0x0f => {
                w.varint(r.varint()?);
                w.u8(r.u8()?);
                w.bytes(r.bytes(21)?); // XYZ, yaw/pitch/head, three velocities
                upgrade_metadata(&mut r, &mut w)?;
            }
            0x12 => {
                w.varint(r.i32()?);
                w.bytes(r.bytes(6)?);
            }
            0x13 => {
                let count = r.u8()?;
                w.varint(i32::from(count));
                for _ in 0..count {
                    w.varint(r.i32()?);
                }
            }
            0x15..=0x17 => {
                w.varint(r.i32()?);
                let len = match id {
                    0x15 => 3,
                    0x16 => 2,
                    _ => 5,
                };
                w.bytes(r.bytes(len)?);
                w.bool(false); // 1.7 does not transmit on-ground for other entities.
            }
            0x18 => {
                w.varint(r.i32()?);
                w.bytes(r.bytes(14)?);
                w.bool(false);
            }
            0x19 => {
                w.varint(r.i32()?);
                w.u8(r.u8()?);
            }
            0x1a => {
                w.i32(r.i32()?);
                w.i8(r.i8()?);
            }
            0x1c => {
                w.varint(r.i32()?);
                upgrade_metadata(&mut r, &mut w)?;
            }
            0x1d => {
                w.varint(r.i32()?);
                w.u8(r.u8()?);
                w.u8(r.u8()?);
                w.varint(i32::from(r.i16()?));
                w.bool(false);
            }
            0x1e => {
                w.varint(r.i32()?);
                w.u8(r.u8()?);
            }
            0x1f => {
                w.f32(r.f32()?);
                w.varint(i32::from(r.i16()?));
                w.varint(i32::from(r.i16()?));
            }
            0x20 => {
                w.varint(r.i32()?);
                let count = r.i32()?;
                if !(0..=1024).contains(&count) {
                    return Err(Error::new("invalid 1.7 attribute count"));
                }
                w.i32(count);
                for _ in 0..count {
                    w.string(&r.string(64)?);
                    w.f64(r.f64()?);
                    let modifiers = r.i16()?;
                    if !(0..=1024).contains(&modifiers) {
                        return Err(Error::new("invalid 1.7 modifier count"));
                    }
                    w.varint(i32::from(modifiers));
                    for _ in 0..modifiers {
                        w.bytes(r.bytes(25)?);
                    }
                }
            }
            0x21 => {
                let column = chunk::Column::decode(bytes, self.skylight)?;
                return Ok(vec![(0x21, column.to_protocol47_payload())]);
            }
            0x22 => {
                w.i32(r.i32()?);
                w.i32(r.i32()?);
                let count = usize::from(r.u16()?);
                let length = r.i32()?;
                if length < 0 || length as usize != count * 4 || r.remaining() != count * 4 {
                    return Err(Error::new("invalid 1.7 multi-block length"));
                }
                w.varint(count as i32);
                for _ in 0..count {
                    w.u16(r.u16()?);
                    w.varint(i32::from(r.u16()?));
                }
            }
            0x23 => {
                let (x, y, z) = (r.i32()?, i32::from(r.u8()?), r.i32()?);
                let block = r.varint()?;
                let meta = r.u8()?;
                if !(0..=4095).contains(&block) || meta > 15 {
                    return Err(Error::new("invalid 1.7 block state"));
                }
                w.i64(legacy18::actions::block_position(x, y, z)? as i64);
                w.varint((block << 4) | i32::from(meta));
            }
            0x26 => {
                let mut result = Vec::new();
                let mut total = 0usize;
                for column in chunk::decode_bulk(bytes)? {
                    let payload = column.to_protocol47_payload();
                    total = total
                        .checked_add(payload.len())
                        .ok_or_else(|| Error::new("1.7 chunk conversion overflow"))?;
                    if total > crate::frame::MAX_UNCOMPRESSED {
                        return Err(Error::new(
                            "1.7 bulk chunk expansion exceeds the packet budget",
                        ));
                    }
                    result.push((0x21, payload));
                }
                return Ok(result);
            }
            0x2b => {
                w.u8(r.u8()?);
                w.f32(r.f32()?);
            }
            0x2d => {
                w.u8(r.u8()?);
                let kind = r.u8()?;
                let title = r.string(32767)?;
                let slots = r.u8()?;
                let custom_title = r.bool()?;
                let (name, default_title) = match kind {
                    0 => ("minecraft:chest", "container.chest"),
                    1 => ("minecraft:crafting_table", "container.crafting"),
                    2 => ("minecraft:furnace", "container.furnace"),
                    3 => ("minecraft:dispenser", "container.dispenser"),
                    4 => ("minecraft:enchanting_table", "container.enchant"),
                    5 => ("minecraft:brewing_stand", "container.brewing"),
                    6 => ("minecraft:villager", "entity.Villager.name"),
                    7 => ("minecraft:beacon", "container.beacon"),
                    8 => ("minecraft:anvil", "container.repair"),
                    9 => ("minecraft:hopper", "container.hopper"),
                    10 => ("minecraft:dropper", "container.dropper"),
                    11 => ("EntityHorse", "entity.horse.name"),
                    _ => return Err(Error::new("unknown 1.7 window type")),
                };
                let horse = if kind == 11 { Some(r.i32()?) } else { None };
                w.string(name);
                let title = if custom_title {
                    serde_json::json!({"text":title})
                } else {
                    serde_json::json!({"translate":default_title})
                };
                w.string(&title.to_string());
                // Non-storage workstations use zero in the renderer's 1.8
                // representation; 1.7 sends their actual inventory size.
                w.u8(if matches!(kind, 1 | 4 | 8) { 0 } else { slots });
                if let Some(entity) = horse {
                    w.i32(entity);
                }
            }
            0x2e => w.u8(r.u8()?),
            0x2f => {
                w.i8(r.i8()?);
                w.i16(r.i16()?);
                upgrade_slot(&mut r, &mut w)?;
            }
            0x30 => {
                w.u8(r.u8()?);
                let count = r.i16()?;
                if !(0..=4096).contains(&count) {
                    return Err(Error::new("invalid 1.7 inventory count"));
                }
                w.i16(count);
                for _ in 0..count {
                    upgrade_slot(&mut r, &mut w)?;
                }
            }
            0x31 => {
                w.u8(r.u8()?);
                w.i16(r.i16()?);
                w.i16(r.i16()?);
            }
            0x32 => {
                w.u8(r.u8()?);
                w.i16(r.i16()?);
                w.bool(r.bool()?);
            }
            0x38 => {
                let name = r.string(16)?;
                let online = r.bool()?;
                let ping = i32::from(r.i16()?).max(0);
                finish(&r)?;
                let known = self.players.get(&name).copied();
                let uuid = known.unwrap_or_else(|| crate::crypto::offline_uuid(&name));
                let data = if !online {
                    self.players.remove(&name);
                    let mut gone = Writer::new();
                    gone.varint(4);
                    gone.varint(1);
                    gone.uuid(uuid);
                    gone.data
                } else if known.is_some() {
                    // 1.7 reuses this packet for latency. Re-adding the profile
                    // here would erase textures supplied by Spawn Player.
                    let mut latency = Writer::new();
                    latency.varint(2);
                    latency.varint(1);
                    latency.uuid(uuid);
                    latency.varint(ping);
                    latency.data
                } else {
                    if self.players.len() >= 4096 {
                        return Err(Error::new("1.7 player list exceeds the session budget"));
                    }
                    self.players.insert(name.clone(), uuid);
                    player_info(uuid, &name, ping, &[])
                };
                return Ok(vec![(0x38, data)]);
            }
            0x39 => {
                w.u8(r.u8()?);
                w.f32(r.f32()?);
                w.f32(r.f32()?);
            }
            0x3b => {
                let name = r.string(16)?;
                let display = r.string(32)?;
                let method = r.u8()?;
                if method > 2 {
                    return Err(Error::new("invalid 1.7 objective mode"));
                }
                w.string(&name);
                w.u8(method);
                if method != 1 {
                    w.string(&display);
                    w.string("integer");
                }
            }
            0x3c => {
                w.string(&r.string(40)?);
                let action = r.u8()?;
                w.u8(action);
                match action {
                    0 => {
                        w.string(&r.string(16)?);
                        w.varint(r.i32()?);
                    }
                    1 => w.string(""),
                    _ => return Err(Error::new("invalid 1.7 score action")),
                }
            }
            0x3d => {
                w.u8(r.u8()?);
                w.string(&r.string(16)?);
            }
            0x3e => {
                w.string(&r.string(16)?);
                let method = r.u8()?;
                if method > 4 {
                    return Err(Error::new("invalid 1.7 team mode"));
                }
                w.u8(method);
                if method == 0 || method == 2 {
                    w.string(&r.string(32)?);
                    w.string(&r.string(16)?);
                    w.string(&r.string(16)?);
                    w.u8(r.u8()?);
                    w.string("always");
                    w.i8(-1);
                }
                if matches!(method, 0 | 3 | 4) {
                    let count = r.i16()?;
                    if !(0..=4096).contains(&count) {
                        return Err(Error::new("invalid 1.7 team size"));
                    }
                    w.varint(i32::from(count));
                    for _ in 0..count {
                        w.string(&r.string(40)?);
                    }
                }
            }
            0x40 => w.string(&r.string(32767)?),
            _ => return Ok(Vec::new()),
        }
        finish(&r)?;
        // Validate shared typed packets as well as their legacy framing.
        legacy18::play::Packet::decode(id, &w.data)?;
        Ok(vec![(id, w.data)])
    }

    fn player_spawn(&mut self, r: &mut Reader<'_>) -> Result<Vec<RenderPacket>> {
        let entity = r.varint()?;
        let uuid = parse_uuid(&r.string(36)?)?;
        let name = r.string(16)?;
        let mut properties = Vec::new();
        if self.version == Version::V1_7_10 {
            let count = r.count(64)?;
            for _ in 0..count {
                properties.push((r.string(32767)?, r.string(32767)?, r.string(32767)?));
            }
        }
        let mut spawn = Writer::new();
        spawn.varint(entity);
        spawn.uuid(uuid);
        spawn.bytes(r.bytes(16)?);
        upgrade_metadata(r, &mut spawn)?;
        finish(r)?;
        let mut result = Vec::new();
        if !self.players.contains_key(&name) && self.players.len() >= 4096 {
            return Err(Error::new("1.7 player list exceeds the session budget"));
        }
        if let Some(old) = self.players.insert(name.clone(), uuid)
            && old != uuid
        {
            let mut remove = Writer::new();
            remove.varint(4);
            remove.varint(1);
            remove.uuid(old);
            result.push((0x38, remove.data));
        }
        result.push((0x38, player_info(uuid, &name, 0, &properties)));
        result.push((0x0c, spawn.data));
        Ok(result)
    }
}

fn player_info(
    uuid: u128,
    name: &str,
    ping: i32,
    properties: &[(String, String, String)],
) -> Vec<u8> {
    let mut w = Writer::new();
    w.varint(0);
    w.varint(1);
    w.uuid(uuid);
    w.string(name);
    w.varint(properties.len() as i32);
    for (name, value, signature) in properties {
        w.string(name);
        w.string(value);
        w.bool(!signature.is_empty());
        if !signature.is_empty() {
            w.string(signature);
        }
    }
    w.varint(0);
    w.varint(ping);
    w.bool(false);
    w.data
}

fn parse_uuid(value: &str) -> Result<u128> {
    if !matches!(value.len(), 32 | 36)
        || !value.bytes().enumerate().all(|(i, b)| {
            if value.len() == 36 && [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
    {
        return Err(Error::new("invalid 1.7 player UUID"));
    }
    u128::from_str_radix(&value.replace('-', ""), 16)
        .map_err(|_| Error::new("invalid 1.7 player UUID"))
}

fn upgrade_slot(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    legacy18::slot::Slot::write(slot::read(r)?.as_ref(), w)?;
    if w.data.len() > crate::frame::MAX_UNCOMPRESSED {
        return Err(Error::new(
            "1.7 inventory expansion exceeds the packet budget",
        ));
    }
    Ok(())
}

fn downgrade_slot(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    slot::write(legacy18::slot::Slot::read(r)?.as_ref(), w)
}

fn upgrade_metadata(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    let mut seen = 0u32;
    loop {
        let header = r.u8()?;
        if header == 0x7f {
            w.u8(header);
            return Ok(());
        }
        // The living-entity custom name/visibility moved into common metadata
        // in 1.8. Item-stack field 10 has another serializer and stays put.
        let index = match (header >> 5, header & 31) {
            (4, 10) => 2,
            (0, 11) => 3,
            (_, index) => index,
        };
        w.u8((header & 0xe0) | index);
        let bit = 1u32 << index;
        if seen & bit != 0 {
            return Err(Error::new("duplicate 1.7 metadata field"));
        }
        seen |= bit;
        match header >> 5 {
            0 => w.i8(r.i8()?),
            1 => w.i16(r.i16()?),
            2 => w.i32(r.i32()?),
            3 => w.f32(r.f32()?),
            4 => w.string(&r.string(32767)?),
            5 => upgrade_slot(r, w)?,
            6 => w.bytes(r.bytes(12)?),
            _ => return Err(Error::new("unknown 1.7 metadata type")),
        }
    }
}

pub fn finish(r: &Reader<'_>) -> Result<()> {
    if r.remaining() != 0 {
        return Err(Error::new("trailing 1.7 packet data"));
    }
    Ok(())
}

/// Re-encode a packet from the existing action model for a 1.7 server.
/// This accepts only packet kinds the shared action encoder actually produces.
pub fn outgoing(bytes: &[u8], entity: i32) -> Result<Vec<u8>> {
    let mut r = Reader::new(bytes);
    let id = r.varint()?;
    let mut w = Writer::new();
    w.varint(id);
    match id {
        0x00 => w.i32(r.varint()?),
        0x01 => w.string(&r.string(100)?),
        0x02 => {
            w.i32(r.varint()?);
            let action = r.varint()?;
            if !(0..=1).contains(&action) {
                return Err(Error::new("unsupported 1.7 entity interaction"));
            }
            w.u8(action as u8);
        }
        0x03 => w.bool(r.bool()?),
        0x04 | 0x06 => {
            let (x, y, z) = (r.f64()?, r.f64()?, r.f64()?);
            if ![x, y, z].iter().all(|v| v.is_finite()) {
                return Err(Error::new("invalid 1.7 movement"));
            }
            w.f64(x);
            w.f64(y);
            w.f64(y + f64::from(1.62_f32));
            w.f64(z);
            if id == 0x06 {
                w.f32(r.f32()?);
                w.f32(r.f32()?);
            }
            w.bool(r.bool()?);
        }
        0x05 => {
            w.f32(r.f32()?);
            w.f32(r.f32()?);
            w.bool(r.bool()?);
        }
        0x07 => {
            w.u8(r.u8()?);
            unpack_position(&mut r, &mut w, false)?;
            w.u8(r.u8()?);
        }
        0x08 => {
            unpack_position(&mut r, &mut w, true)?;
            w.u8(r.u8()?);
            downgrade_slot(&mut r, &mut w)?;
            w.bytes(r.bytes(3)?);
        }
        0x09 => w.i16(r.i16()?),
        0x0a => {
            w.i32(entity);
            w.u8(1);
        }
        0x0b => {
            w.i32(r.varint()?);
            let action = r.varint()?;
            if !(0..=6).contains(&action) {
                return Err(Error::new("invalid 1.7 entity action"));
            }
            w.u8(action as u8 + 1);
            w.i32(r.varint()?);
        }
        0x0d => w.u8(r.u8()?),
        0x0e => {
            w.u8(r.u8()?);
            w.i16(r.i16()?);
            w.u8(r.u8()?);
            w.i16(r.i16()?);
            w.u8(r.u8()?);
            downgrade_slot(&mut r, &mut w)?;
        }
        0x0f => {
            w.u8(r.u8()?);
            w.i16(r.i16()?);
            w.bool(r.bool()?);
        }
        0x10 => {
            w.i16(r.i16()?);
            downgrade_slot(&mut r, &mut w)?;
        }
        0x13 => {
            w.u8(r.u8()?);
            w.f32(r.f32()?);
            w.f32(r.f32()?);
        }
        0x15 => {
            w.string(&r.string(7)?);
            w.i8(r.i8()?);
            w.u8(r.u8()?);
            w.bool(r.bool()?);
            let skin = r.u8()?;
            w.u8(2); // Historical difficulty preference; server remains authoritative.
            w.bool(skin & 1 != 0);
        }
        0x16 => {
            let status = r.varint()?;
            if !(0..=2).contains(&status) {
                return Err(Error::new("invalid 1.7 client status"));
            }
            w.u8(status as u8);
        }
        _ => {
            return Err(Error::new(format!(
                "action {id:#x} is not implemented for 1.7"
            )));
        }
    }
    finish(&r)?;
    Ok(w.data)
}

fn unpack_position(r: &mut Reader<'_>, w: &mut Writer, allow_use: bool) -> Result<()> {
    let packed = r.i64()?;
    if allow_use && packed == -1 {
        w.i32(-1);
        w.u8(255);
        w.i32(-1);
        return Ok(());
    }
    let x = (packed >> 38) as i32;
    let y = ((packed as u64 >> 26) & 4095) as u16;
    let z = (packed << 38 >> 38) as i32;
    if y > 255 {
        return Err(Error::new("block outside 1.7 build height"));
    }
    w.i32(x);
    w.u8(y as u8);
    w.i32(z);
    Ok(())
}
