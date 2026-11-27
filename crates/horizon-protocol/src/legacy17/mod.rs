//! Minecraft 1.7.2–1.7.10 transport (protocols 4 and 5).
//!
//! These versions enter Play immediately after login and never negotiate packet
//! compression. Their encryption arrays have two-byte lengths, unlike 1.8.
//! A transport implementation does not provide historical singleplayer rules.
pub mod actions;
pub mod bridge;
pub mod chunk;
pub mod local_flat;
pub mod play;
pub mod presentation;
pub mod slot;

use crate::{
    Error, Result,
    buf::{Reader, Writer},
    connection::Connection,
    crypto::{generate_secret, server_hash},
    packets::handshake,
    rsa::RsaPublicKey,
};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Version {
    /// Minecraft 1.7.2–1.7.5.
    V1_7_2,
    /// Minecraft 1.7.6–1.7.10.
    V1_7_10,
}

impl Version {
    pub const fn release(self) -> &'static str {
        match self {
            Self::V1_7_2 => "1.7.2",
            Self::V1_7_10 => "1.7.10",
        }
    }

    pub const fn protocol(self) -> i32 {
        match self {
            Self::V1_7_2 => 4,
            Self::V1_7_10 => 5,
        }
    }

    pub const fn from_protocol(protocol: i32) -> Option<Self> {
        match protocol {
            4 => Some(Self::V1_7_2),
            5 => Some(Self::V1_7_10),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Profile {
    pub uuid: u128,
    pub name: String,
}

/// Preserve a server's chat component separately from transport errors.
pub enum LoginOutcome {
    Joined(Connection, Profile),
    Disconnected(String),
}

#[derive(Debug, PartialEq, Eq)]
pub enum LoginPacket {
    Disconnect(String),
    Encryption {
        server_id: String,
        public_key: Vec<u8>,
        verify_token: Vec<u8>,
    },
    Success(Profile),
}

impl LoginPacket {
    pub fn decode(id: i32, payload: &[u8]) -> Result<Self> {
        let mut r = Reader::new(payload);
        let packet = match id {
            0 => Self::Disconnect(r.string(32767)?),
            1 => Self::Encryption {
                server_id: r.string(20)?,
                public_key: short_array(&mut r, 8192)?,
                verify_token: short_array(&mut r, 1024)?,
            },
            2 => {
                let uuid = parse_uuid(&r.string(36)?)?;
                let name = r.string(16)?;
                check_name(&name)?;
                Self::Success(Profile { uuid, name })
            }
            _ => {
                return Err(Error::new(format!(
                    "unknown protocol 4/5 login packet {id}"
                )));
            }
        };
        finish(&r)?;
        Ok(packet)
    }
}

fn short_array(r: &mut Reader<'_>, max: usize) -> Result<Vec<u8>> {
    let len = usize::from(r.u16()?);
    if len == 0 || len > max {
        return Err(Error::new("invalid protocol 4/5 encryption array length"));
    }
    Ok(r.bytes(len)?.to_vec())
}

fn write_short_array(w: &mut Writer, data: &[u8]) -> Result<()> {
    let len = i16::try_from(data.len())
        .map_err(|_| Error::new("protocol 4/5 encryption array too long"))?;
    w.i16(len);
    w.bytes(data);
    Ok(())
}

fn parse_uuid(s: &str) -> Result<u128> {
    let compact = match s.len() {
        32 if s.bytes().all(|b| b.is_ascii_hexdigit()) => s.to_owned(),
        36 if s.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        }) =>
        {
            s.replace('-', "")
        }
        _ => return Err(Error::new("invalid protocol 4/5 profile UUID")),
    };
    u128::from_str_radix(&compact, 16).map_err(|_| Error::new("invalid profile UUID"))
}

fn check_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 16
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(Error::new(
            "player name must contain 1–16 ASCII letters, digits or underscores",
        ));
    }
    Ok(())
}

pub(crate) fn finish(r: &Reader<'_>) -> Result<()> {
    if r.remaining() != 0 {
        return Err(Error::new("trailing bytes in protocol 4/5 packet"));
    }
    Ok(())
}

/// Authenticate using the caller's signed-in account when the server requests
/// encryption. Authentication failure closes the connection; it never retries
/// in offline mode. The caller must also bound its session-service HTTP request.
pub fn login(
    version: Version,
    connection: Connection,
    host: &str,
    port: u16,
    name: &str,
    timeout: Duration,
    authenticate: impl FnMut(&str) -> Result<()>,
) -> Result<(Connection, Profile)> {
    match login_outcome(version, connection, host, port, name, timeout, authenticate)? {
        LoginOutcome::Joined(connection, profile) => Ok((connection, profile)),
        LoginOutcome::Disconnected(reason) => {
            Err(Error::new(format!("server disconnected: {reason}")))
        }
    }
}

pub fn login_outcome(
    version: Version,
    mut connection: Connection,
    host: &str,
    port: u16,
    name: &str,
    timeout: Duration,
    mut authenticate: impl FnMut(&str) -> Result<()>,
) -> Result<LoginOutcome> {
    check_name(name)?;
    if host.is_empty() || host.encode_utf16().count() > 255 || host.contains('\0') {
        return Err(Error::new("invalid handshake host"));
    }
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or_else(|| Error::new("invalid login timeout"))?;
    if timeout.is_zero() {
        return Err(Error::timed_out());
    }
    connection.send(
        &handshake::Serverbound::Intention(handshake::Intention {
            protocol: version.protocol(),
            host: host.to_owned(),
            port,
            intent: handshake::LOGIN,
        })
        .encode(),
    )?;
    let mut hello = Writer::new();
    hello.varint(0);
    hello.string(name);
    connection.send(&hello.data)?;

    let mut encrypted = false;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(Error::timed_out());
        }
        connection.reader.set_read_timeout(Some(remaining))?;
        let (id, payload) = connection.recv()?;
        match LoginPacket::decode(id, &payload)? {
            LoginPacket::Disconnect(reason) => return Ok(LoginOutcome::Disconnected(reason)),
            LoginPacket::Success(profile) => {
                connection
                    .reader
                    .set_read_timeout(Some(Duration::from_secs(30)))?;
                return Ok(LoginOutcome::Joined(connection, profile));
            }
            LoginPacket::Encryption {
                server_id,
                public_key,
                verify_token,
            } => {
                if encrypted {
                    return Err(Error::new("repeated protocol 4/5 encryption request"));
                }
                let key = RsaPublicKey::from_der(&public_key)?;
                let secret = generate_secret()?;
                authenticate(&server_hash(&server_id, &secret, &public_key))?;
                if Instant::now() >= deadline {
                    return Err(Error::timed_out());
                }
                let mut response = Writer::new();
                response.varint(1);
                write_short_array(&mut response, &key.encrypt(&secret)?)?;
                write_short_array(&mut response, &key.encrypt(&verify_token)?)?;
                connection.send(&response.data)?;
                connection.enable_encryption(&secret);
                encrypted = true;
            }
        }
    }
}
