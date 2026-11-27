//! Player actions with protocol 4/5 field widths and packet IDs.
use super::{
    play::Pose,
    slot::{self, Slot},
};
use crate::{Error, Result, buf::Writer};

#[derive(Clone, Copy, Debug)]
pub enum Digging {
    Start = 0,
    Cancel = 1,
    Finish = 2,
    DropStack = 3,
    DropOne = 4,
    ReleaseUse = 5,
}

/// Action values are one-based in 1.7, not the zero-based 1.8 values.
#[derive(Clone, Copy, Debug)]
pub enum EntityAction {
    StartSneaking = 1,
    StopSneaking = 2,
    LeaveBed = 3,
    StartSprinting = 4,
    StopSprinting = 5,
    HorseJump = 6,
    OpenHorseInventory = 7,
}

pub fn digging(action: Digging, x: i32, y: i32, z: i32, face: u8) -> Result<Vec<u8>> {
    if face > 5 || !(0..256).contains(&y) {
        return Err(Error::new("invalid protocol 4/5 digging position or face"));
    }
    let mut w = Writer::new();
    w.varint(0x07);
    w.u8(action as u8);
    w.i32(x);
    w.u8(y as u8);
    w.i32(z);
    w.u8(face);
    Ok(w.data)
}

pub fn interact_entity(entity: i32, attack: bool) -> Result<Vec<u8>> {
    if entity < 0 {
        return Err(Error::new("negative protocol 4/5 entity ID"));
    }
    let mut w = Writer::new();
    w.varint(0x02);
    w.i32(entity);
    w.u8(u8::from(attack));
    Ok(w.data)
}

pub fn entity_action(entity: i32, action: EntityAction, jump_boost: i32) -> Result<Vec<u8>> {
    if entity < 0
        || !(0..=100).contains(&jump_boost)
        || (!matches!(action, EntityAction::HorseJump) && jump_boost != 0)
    {
        return Err(Error::new("invalid protocol 4/5 entity action"));
    }
    let mut w = Writer::new();
    w.varint(0x0b);
    w.i32(entity);
    w.u8(action as u8);
    w.i32(jump_boost);
    Ok(w.data)
}

pub fn held_slot(slot: u8) -> Result<Vec<u8>> {
    if slot > 8 {
        return Err(Error::new("hotbar slot must be 0 through 8"));
    }
    let mut w = Writer::new();
    w.varint(0x09);
    w.i16(i16::from(slot));
    Ok(w.data)
}

pub fn swing_arm(entity: i32) -> Result<Vec<u8>> {
    if entity < 0 {
        return Err(Error::new("negative protocol 4/5 entity ID"));
    }
    let mut w = Writer::new();
    w.varint(0x0a);
    w.i32(entity);
    w.u8(1);
    Ok(w.data)
}

pub fn respawn() -> Vec<u8> {
    vec![0x16, 0]
}
pub fn ground(on_ground: bool) -> Vec<u8> {
    vec![0x03, u8::from(on_ground)]
}

pub fn position(x: f64, y: f64, z: f64, on_ground: bool) -> Result<Vec<u8>> {
    Pose {
        x,
        y,
        z,
        ..Pose::default()
    }
    .validate()?;
    let mut w = Writer::new();
    w.varint(4);
    w.f64(x);
    w.f64(y);
    w.f64(y + super::play::EYE_HEIGHT);
    w.f64(z);
    w.bool(on_ground);
    Ok(w.data)
}

pub fn look(yaw: f32, pitch: f32, on_ground: bool) -> Result<Vec<u8>> {
    Pose {
        yaw,
        pitch,
        ..Pose::default()
    }
    .validate()?;
    let mut w = Writer::new();
    w.varint(5);
    w.f32(yaw);
    w.f32(pitch);
    w.bool(on_ground);
    Ok(w.data)
}

pub fn place(
    x: i32,
    y: i32,
    z: i32,
    face: u8,
    held: Option<&Slot>,
    cursor: [u8; 3],
) -> Result<Vec<u8>> {
    if !(0..256).contains(&y) || face > 5 || cursor.iter().any(|&v| v > 16) {
        return Err(Error::new("invalid protocol 4/5 placement"));
    }
    let mut w = Writer::new();
    w.varint(8);
    w.i32(x);
    w.u8(y as u8);
    w.i32(z);
    w.u8(face);
    slot::write(held, &mut w)?;
    w.bytes(&cursor);
    Ok(w.data)
}

/// Right-clicking the held item uses the legacy -1 position/face sentinel.
pub fn use_item(held: Option<&Slot>) -> Result<Vec<u8>> {
    let mut w = Writer::new();
    w.varint(8);
    w.i32(-1);
    w.u8(255);
    w.i32(-1);
    w.u8(255);
    slot::write(held, &mut w)?;
    w.bytes(&[0, 0, 0]);
    Ok(w.data)
}

pub fn creative_slot(index: i16, item: Option<&Slot>) -> Result<Vec<u8>> {
    if !(-1..=44).contains(&index) {
        return Err(Error::new("invalid protocol 4/5 creative inventory slot"));
    }
    let mut w = Writer::new();
    w.varint(0x10);
    w.i16(index);
    slot::write(item, &mut w)?;
    Ok(w.data)
}

pub fn client_settings(
    locale: &str,
    view_distance: i8,
    chat_mode: u8,
    chat_colors: bool,
    difficulty: u8,
    show_cape: bool,
) -> Result<Vec<u8>> {
    if locale.is_empty()
        || locale.encode_utf16().count() > 7
        || !(1..=32).contains(&view_distance)
        || chat_mode > 2
        || difficulty > 3
    {
        return Err(Error::new("invalid protocol 4/5 client settings"));
    }
    let mut w = Writer::new();
    w.varint(0x15);
    w.string(locale);
    w.i8(view_distance);
    w.u8(chat_mode);
    w.bool(chat_colors);
    w.u8(difficulty);
    w.bool(show_cape);
    Ok(w.data)
}

pub fn plugin_message(channel: &str, payload: &[u8]) -> Result<Vec<u8>> {
    if channel.is_empty() || channel.encode_utf16().count() > 20 {
        return Err(Error::new("invalid protocol 4/5 plugin channel"));
    }
    let length = i16::try_from(payload.len())
        .map_err(|_| Error::new("protocol 4/5 plugin message too long"))?;
    let mut w = Writer::new();
    w.varint(0x17);
    w.string(channel);
    w.i16(length);
    w.bytes(payload);
    Ok(w.data)
}

/// Four plain lines, with short Y, for the front of a 1.7 sign.
pub fn sign_update(position: [i32; 3], lines: &[String; 4]) -> Result<Vec<u8>> {
    super::presentation::check_sign_position(position)?;
    if lines.iter().any(|line| line.encode_utf16().count() > 15) {
        return Err(Error::new("protocol 4/5 sign line exceeds 15 UTF-16 units"));
    }
    let mut w = Writer::new();
    w.varint(0x12);
    w.i32(position[0]);
    w.i16(position[1] as i16);
    w.i32(position[2]);
    for line in lines {
        w.string(line);
    }
    Ok(w.data)
}
