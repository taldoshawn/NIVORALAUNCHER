//! Protocol 4/5 Play packets. Coordinates exposed to the engine use feet Y;
//! 1.7's clientbound position instead carries the player's eye/stance Y.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
};

/// The 1.7 client stores its standing eye offset as a Java float.
pub const EYE_HEIGHT: f64 = 1.62_f32 as f64;

#[derive(Debug, PartialEq)]
pub enum Packet<'a> {
    KeepAlive(i32),
    Join {
        entity: i32,
        gamemode: u8,
        dimension: i8,
        difficulty: u8,
        max_players: u8,
        level_type: String,
    },
    Chat {
        json: String,
    },
    Time {
        game_time: i64,
        day_time: i64,
        ticking: bool,
    },
    SpawnPosition {
        x: i32,
        y: i32,
        z: i32,
    },
    Health {
        health: f32,
        food: i16,
        saturation: f32,
    },
    Respawn {
        dimension: i32,
        difficulty: u8,
        gamemode: u8,
        level_type: String,
    },
    Position {
        x: f64,
        y: f64,
        z: f64,
        yaw: f32,
        pitch: f32,
        on_ground: bool,
    },
    HeldSlot(u8),
    Velocity {
        entity: i32,
        x: i16,
        y: i16,
        z: i16,
    },
    BlockChange {
        x: i32,
        y: u8,
        z: i32,
        state: u16,
    },
    Chunk(&'a [u8]),
    ChunkBulk(&'a [u8]),
    Disconnect(String),
    Unknown {
        id: i32,
        payload: &'a [u8],
    },
}

impl<'a> Packet<'a> {
    pub fn decode(id: i32, payload: &'a [u8]) -> Result<Self> {
        if id < 0 {
            return Err(Error::new("negative protocol 4/5 packet ID"));
        }
        let mut r = Reader::new(payload);
        let packet = match id {
            0x00 => Self::KeepAlive(r.i32()?),
            0x01 => Self::Join {
                entity: r.i32()?,
                gamemode: r.u8()?,
                dimension: r.i8()?,
                difficulty: r.u8()?,
                max_players: r.u8()?,
                level_type: r.string(16)?,
            },
            0x02 => Self::Chat {
                json: r.string(32767)?,
            },
            0x03 => {
                let game_time = r.i64()?;
                let signed_day = r.i64()?;
                Self::Time {
                    game_time,
                    day_time: signed_day
                        .checked_abs()
                        .ok_or_else(|| Error::new("invalid protocol 4/5 day time"))?,
                    ticking: signed_day >= 0,
                }
            }
            0x05 => Self::SpawnPosition {
                x: r.i32()?,
                y: r.i32()?,
                z: r.i32()?,
            },
            0x06 => {
                let (health, food, saturation) = (r.f32()?, r.i16()?, r.f32()?);
                if !health.is_finite() || !saturation.is_finite() || food < 0 {
                    return Err(Error::new("invalid protocol 4/5 health update"));
                }
                Self::Health {
                    health,
                    food,
                    saturation,
                }
            }
            0x07 => Self::Respawn {
                dimension: r.i32()?,
                difficulty: r.u8()?,
                gamemode: r.u8()?,
                level_type: r.string(16)?,
            },
            0x08 => {
                let (x, eye_y, z, yaw, pitch, on_ground) =
                    (r.f64()?, r.f64()?, r.f64()?, r.f32()?, r.f32()?, r.bool()?);
                let pose = Pose {
                    x,
                    y: eye_y - EYE_HEIGHT,
                    z,
                    yaw,
                    pitch,
                };
                pose.validate()?;
                Self::Position {
                    x,
                    y: pose.y,
                    z,
                    yaw,
                    pitch,
                    on_ground,
                }
            }
            0x09 => {
                let slot = r.u8()?;
                if slot > 8 {
                    return Err(Error::new("invalid protocol 4/5 held slot"));
                }
                Self::HeldSlot(slot)
            }
            0x12 => Self::Velocity {
                entity: r.i32()?,
                x: r.i16()?,
                y: r.i16()?,
                z: r.i16()?,
            },
            0x21 => Self::Chunk(r.rest()),
            0x23 => {
                let (x, y, z, id, metadata) = (r.i32()?, r.u8()?, r.i32()?, r.varint()?, r.u8()?);
                if !(0..4096).contains(&id) || metadata > 15 {
                    return Err(Error::new("invalid protocol 4/5 block state"));
                }
                Self::BlockChange {
                    x,
                    y,
                    z,
                    state: (id as u16) << 4 | u16::from(metadata),
                }
            }
            0x26 => Self::ChunkBulk(r.rest()),
            0x40 => Self::Disconnect(r.string(32767)?),
            _ => Self::Unknown {
                id,
                payload: r.rest(),
            },
        };
        super::finish(&r)?;
        Ok(packet)
    }
}

pub fn keep_alive(id: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.varint(0);
    w.i32(id);
    w.data
}

pub fn chat(message: &str) -> Result<Vec<u8>> {
    if message.encode_utf16().count() > 100
        || message
            .chars()
            .any(|c| c < ' ' || c == '\u{7f}' || c == '§')
    {
        return Err(Error::new("invalid or overlong protocol 4/5 chat message"));
    }
    let mut w = Writer::new();
    w.varint(1);
    w.string(message);
    Ok(w.data)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
}

impl Pose {
    /// A 1.7 correction is absolute, with no relative-position flags or ID.
    pub fn corrected(self, packet: &Packet<'_>) -> Result<Self> {
        let Packet::Position {
            x,
            y,
            z,
            yaw,
            pitch,
            ..
        } = packet
        else {
            return Err(Error::new("expected protocol 4/5 position packet"));
        };
        let pose = Self {
            x: *x,
            y: *y,
            z: *z,
            yaw: *yaw,
            pitch: *pitch,
        };
        pose.validate()?;
        Ok(pose)
    }

    pub(crate) fn validate(&self) -> Result<()> {
        if ![
            self.x,
            self.y,
            self.y + EYE_HEIGHT,
            self.z,
            f64::from(self.yaw),
            f64::from(self.pitch),
        ]
        .iter()
        .all(|n| n.is_finite())
        {
            return Err(Error::new("non-finite protocol 4/5 player position"));
        }
        Ok(())
    }

    /// Serverbound Player Position and Look includes both feet and stance Y.
    pub fn encode(self, on_ground: bool) -> Result<Vec<u8>> {
        self.validate()?;
        let mut w = Writer::new();
        w.varint(0x06);
        w.f64(self.x);
        w.f64(self.y);
        w.f64(self.y + EYE_HEIGHT);
        w.f64(self.z);
        w.f32(self.yaw);
        w.f32(self.pitch);
        w.bool(on_ground);
        Ok(w.data)
    }
}
