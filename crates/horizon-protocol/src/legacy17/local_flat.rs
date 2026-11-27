//! Restricted native protocol-4/5 creative-flat preview over an in-memory pipe.
//! Nine fixed hotbar blocks, full-cube collision and bounded terrain editing;
//! no mobs, drops, recipes, survival or normal terrain generation.
mod player;
use super::{
    Version, finish,
    play::{EYE_HEIGHT, Pose},
    slot::{self, Slot},
};
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    connection::{Connection, Pipe},
    crypto::{offline_uuid, uuid_string},
};
use horizon_core::{
    geometry::{Aabb, Vec3},
    position::{BlockPos, ChunkPos},
};
use horizon_world::{
    compression::Compression,
    legacy17_flat::{self, World},
    nbt::Tag,
};
use player::Player;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const HOTBAR: [u8; 9] = [1, 3, 4, 5, 20, 45, 24, 22, 42];
const MAX_PACKET: usize = 64 * 1024;
const KEEPALIVE: Duration = Duration::from_secs(2);

pub fn open(directory: PathBuf, version: Version) -> Result<Pipe> {
    let world_version = match version {
        Version::V1_7_2 => legacy17_flat::Version::V1_7_2,
        Version::V1_7_10 => legacy17_flat::Version::V1_7_10,
    };
    let world = if directory.join("nivora-legacy17-flat.json").exists() {
        World::open(&directory, world_version)?
    } else {
        World::create(&directory, world_version, "Nivora 1.7 creative flat")?
    };
    let (client, server) = Pipe::pair();
    std::thread::Builder::new()
        .name("nivora-flat-17".into())
        .spawn(move || {
            let mut connection = Connection::local(server);
            let mut in_play = false;
            if let Err(error) = run(&mut connection, world, &directory, version, &mut in_play) {
                let mut message = packet(if in_play { 0x40 } else { 0 });
                message.string(&serde_json::json!({"text": error.to_string()}).to_string());
                let _ = connection.send(&message.data);
            }
            connection.writer.shutdown();
        })?;
    Ok(client)
}
fn packet(id: i32) -> Writer {
    let mut w = Writer::new();
    w.varint(id);
    w
}
fn boolean(r: &mut Reader<'_>) -> Result<bool> {
    match r.u8()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(Error::new("invalid 1.7 boolean")),
    }
}
fn slot_value(index: u8) -> Slot {
    Slot {
        id: i16::from(HOTBAR[index as usize]),
        count: 64,
        damage: 0,
        tag: Tag::End,
    }
}
fn inventory(conn: &mut Connection) -> Result<()> {
    let mut w = packet(0x30);
    w.u8(0);
    w.i16(45);
    for index in 0..45 {
        slot::write(
            (index >= 36).then(|| slot_value(index - 36)).as_ref(),
            &mut w,
        )?;
    }
    conn.send(&w.data)?;
    let mut cursor = packet(0x2f);
    cursor.i8(-1);
    cursor.i16(-1);
    slot::write(None, &mut cursor)?;
    conn.send(&cursor.data)
}
fn abilities(conn: &mut Connection, player: &Player) -> Result<()> {
    let mut w = packet(0x39);
    w.u8(13 | if player.flying { 2 } else { 0 });
    w.f32(0.05);
    w.f32(0.1);
    conn.send(&w.data)
}
fn teleport(conn: &mut Connection, player: &Player) -> Result<()> {
    let mut w = packet(8);
    w.f64(player.pose.x);
    w.f64(player.pose.y + EYE_HEIGHT);
    w.f64(player.pose.z);
    w.f32(player.pose.yaw);
    w.f32(player.pose.pitch);
    w.bool(player.on_ground);
    conn.send(&w.data)
}
fn held(conn: &mut Connection, player: &Player) -> Result<()> {
    let mut w = packet(9);
    w.u8(player.held);
    conn.send(&w.data)
}
fn block_at(world: &World, pos: BlockPos) -> Result<u8> {
    if !(0..256).contains(&pos.y) {
        return Ok(0);
    }
    Ok(world.column(pos.chunk())?.block(pos)?)
}
fn send_block(conn: &mut Connection, world: &World, pos: BlockPos) -> Result<()> {
    if !(0..256).contains(&pos.y) {
        return Ok(());
    }
    let mut w = packet(0x23);
    w.i32(pos.x);
    w.u8(pos.y as u8);
    w.i32(pos.z);
    w.varint(i32::from(block_at(world, pos)?));
    w.u8(0);
    conn.send(&w.data)
}
fn array<'a>(tag: &'a Tag, name: &str, length: usize) -> Result<&'a [u8]> {
    match tag.get(name) {
        Some(Tag::ByteArray(v)) if v.len() == length => Ok(v),
        _ => Err(Error::new("invalid native 1.7 section array")),
    }
}
fn send_column(conn: &mut Connection, world: &World, pos: ChunkPos) -> Result<()> {
    let root = world.column(pos)?.into_nbt();
    let level = root
        .tag
        .get("Level")
        .ok_or_else(|| Error::new("missing native 1.7 level"))?;
    let Some(Tag::List(sections)) = level.get("Sections") else {
        return Err(Error::new("missing native 1.7 sections"));
    };
    let mut ordered = [None; 16];
    let mut mask = 0u16;
    for section in sections {
        let Some(Tag::Byte(y @ 0..=15)) = section.get("Y") else {
            return Err(Error::new("invalid native 1.7 section Y"));
        };
        ordered[*y as usize] = Some(section);
        mask |= 1u16 << *y;
    }
    let mut bytes = Vec::with_capacity(sections.len() * 10240 + 256);
    for (field, length) in [
        ("Blocks", 4096),
        ("Data", 2048),
        ("BlockLight", 2048),
        ("SkyLight", 2048),
    ] {
        for section in ordered.iter().flatten() {
            bytes.extend_from_slice(array(section, field, length)?);
        }
    }
    bytes.extend_from_slice(array(level, "Biomes", 256)?);
    let compressed = Compression::Zlib.encode(&bytes)?;
    let mut w = packet(0x21);
    w.i32(pos.x);
    w.i32(pos.z);
    w.bool(true);
    w.u16(mask);
    w.u16(0);
    w.i32(compressed.len() as i32);
    w.bytes(&compressed);
    conn.send(&w.data)
}
fn stream(
    conn: &mut Connection,
    world: &World,
    center: ChunkPos,
    radius: i32,
    loaded: &mut HashSet<ChunkPos>,
) -> Result<()> {
    let mut next = HashSet::new();
    for z in -radius..=radius {
        for x in -radius..=radius {
            next.insert(ChunkPos::new(center.x + x, center.z + z));
        }
    }
    for pos in loaded.difference(&next) {
        let mut w = packet(0x21);
        w.i32(pos.x);
        w.i32(pos.z);
        w.bool(true);
        w.u16(0);
        w.u16(0);
        w.i32(0);
        conn.send(&w.data)?;
    }
    let mut missing: Vec<_> = next.difference(loaded).copied().collect();
    missing.sort_unstable_by_key(|p| (p.x - center.x).pow(2) + (p.z - center.z).pow(2));
    for pos in missing {
        send_column(conn, world, pos)?;
    }
    *loaded = next;
    Ok(())
}
fn body(pose: Pose) -> Aabb {
    let half = f64::from(0.6f32) * 0.5;
    Aabb::new(
        Vec3::new(pose.x - half, pose.y, pose.z - half),
        Vec3::new(pose.x + half, pose.y + f64::from(1.8f32), pose.z + half),
    )
}
fn can_occupy(world: &World, pose: Pose) -> Result<bool> {
    let half = f64::from(0.6f32) * 0.5;
    let bounds = body(pose);
    let mut columns = HashMap::new();
    for x in (pose.x - half).floor() as i32..=(pose.x + half - 1e-7).floor() as i32 {
        for z in (pose.z - half).floor() as i32..=(pose.z + half - 1e-7).floor() as i32 {
            for y in pose.y.floor() as i32..=(pose.y + f64::from(1.8f32) - 1e-7).floor() as i32 {
                if !(0..256).contains(&y) {
                    continue;
                }
                let p = BlockPos::new(x, y, z);
                if let std::collections::hash_map::Entry::Vacant(entry) = columns.entry(p.chunk()) {
                    entry.insert(world.column(p.chunk())?);
                }
                if columns[&p.chunk()].block(p)? != 0 && Aabb::block(p).intersects(bounds) {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}
fn within_reach(player: &Player, pos: BlockPos) -> bool {
    let dx = player.pose.x - (f64::from(pos.x) + 0.5);
    let dy = player.pose.y + EYE_HEIGHT - (f64::from(pos.y) + 0.5);
    let dz = player.pose.z - (f64::from(pos.z) + 0.5);
    dx * dx + dy * dy + dz * dz <= 36.
}
fn changed(
    conn: &mut Connection,
    world: &mut World,
    loaded: &HashSet<ChunkPos>,
    pos: BlockPos,
    id: u8,
) -> Result<()> {
    world.edit(&[(pos, u16::from(id), 0)])?;
    send_block(conn, world, pos)?;
    let center = pos.chunk();
    for z in -1..=1 {
        for x in -1..=1 {
            let p = ChunkPos::new(center.x + x, center.z + z);
            if loaded.contains(&p) {
                send_column(conn, world, p)?;
            }
        }
    }
    Ok(())
}
fn run(
    conn: &mut Connection,
    mut world: World,
    dir: &Path,
    version: Version,
    in_play: &mut bool,
) -> Result<()> {
    conn.reader
        .set_read_timeout(Some(Duration::from_secs(10)))?;
    let (id, data) = conn.recv()?;
    if id != 0 || data.len() > 1024 {
        return Err(Error::new("expected 1.7 local handshake"));
    }
    let mut r = Reader::new(&data);
    if r.varint()? != version.protocol() {
        return Err(Error::new("wrong protocol for this 1.7 world"));
    }
    r.string(255)?;
    r.u16()?;
    if r.varint()? != 2 {
        return Err(Error::new("expected 1.7 login intention"));
    }
    finish(&r)?;
    let (id, data) = conn.recv()?;
    let mut r = Reader::new(&data);
    if id != 0 || data.len() > 64 {
        return Err(Error::new("expected 1.7 login start"));
    }
    let name = r.string(16)?;
    super::check_name(&name)?;
    finish(&r)?;
    let mut player = Player::load(dir, version, &name)?;
    if !can_occupy(&world, player.pose)? {
        return Err(Error::new(
            "saved player intersects terrain; save was preserved",
        ));
    }
    let mut success = packet(2);
    let uuid = offline_uuid(&name);
    success.string(&if version == Version::V1_7_2 {
        format!("{uuid:032x}")
    } else {
        uuid_string(uuid)
    });
    success.string(&name);
    conn.send(&success.data)?;
    *in_play = true;
    let mut join = packet(1);
    join.i32(1);
    join.u8(1);
    join.i8(0);
    join.u8(0);
    join.u8(1);
    join.string("flat");
    conn.send(&join.data)?;
    let mut spawn = packet(5);
    spawn.i32(0);
    spawn.i32(4);
    spawn.i32(0);
    conn.send(&spawn.data)?;
    let mut time = packet(3);
    time.i64(0);
    time.i64(-6000);
    conn.send(&time.data)?;
    abilities(conn, &player)?;
    inventory(conn)?;
    held(conn, &player)?;
    teleport(conn, &player)?;
    let mut center = BlockPos::new(
        player.pose.x.floor() as i32,
        0,
        player.pose.z.floor() as i32,
    )
    .chunk();
    let mut radius = 2;
    let mut loaded = HashSet::new();
    stream(conn, &world, center, radius, &mut loaded)?;
    conn.reader
        .set_read_timeout(Some(Duration::from_millis(100)))?;
    let mut last_keepalive = Instant::now();
    let mut pending = None;
    let mut sequence = 0i32;
    let mut saved = None;
    let result = (|| -> Result<()> {
        loop {
            if last_keepalive.elapsed() >= KEEPALIVE {
                if pending
                    .is_some_and(|(_, at): (i32, Instant)| at.elapsed() > Duration::from_secs(15))
                {
                    return Err(Error::new("1.7 local keepalive timed out"));
                }
                if pending.is_none() {
                    sequence = sequence.wrapping_add(1);
                    conn.send(&super::play::keep_alive(sequence))?;
                    pending = Some((sequence, Instant::now()));
                }
                if saved != Some(player) {
                    player.save(&world, dir, version, &name)?;
                    saved = Some(player);
                }
                last_keepalive = Instant::now();
            }
            let (id, data) = match conn.recv() {
                Ok(value) => value,
                Err(e) if e.is_timed_out() => continue,
                Err(e) if e.is_end_of_stream() => return Ok(()),
                Err(e) => return Err(e),
            };
            if data.len() > MAX_PACKET {
                return Err(Error::new("oversized 1.7 local action"));
            }
            let mut r = Reader::new(&data);
            match id {
                0 => {
                    let value = r.i32()?;
                    finish(&r)?;
                    if pending.is_some_and(|(id, _)| id == value) {
                        pending = None;
                    }
                }
                1 => {
                    r.string(100)?;
                    finish(&r)?;
                }
                2 => {
                    r.i32()?;
                    if r.u8()? > 1 {
                        return Err(Error::new("invalid 1.7 entity interaction"));
                    }
                    finish(&r)?;
                }
                3 => {
                    let ground = boolean(&mut r)?;
                    finish(&r)?;
                    player.on_ground = ground;
                }
                4..=6 => {
                    let mut next = player;
                    if id != 5 {
                        next.pose.x = r.f64()?;
                        next.pose.y = r.f64()?;
                        let stance = r.f64()?;
                        next.pose.z = r.f64()?;
                        if !stance.is_finite() || !(0.1..=1.65).contains(&(stance - next.pose.y)) {
                            return Err(Error::new("invalid 1.7 stance"));
                        }
                    }
                    if id != 4 {
                        next.pose.yaw = r.f32()?;
                        next.pose.pitch = r.f32()?;
                    }
                    next.on_ground = boolean(&mut r)?;
                    finish(&r)?;
                    next.validate()?;
                    let distance = (next.pose.x - player.pose.x).powi(2)
                        + (next.pose.y - player.pose.y).powi(2)
                        + (next.pose.z - player.pose.z).powi(2);
                    if distance > 10000. || !can_occupy(&world, next.pose)? {
                        teleport(conn, &player)?;
                        continue;
                    }
                    player = next;
                    let next = BlockPos::new(
                        player.pose.x.floor() as i32,
                        0,
                        player.pose.z.floor() as i32,
                    )
                    .chunk();
                    if next != center {
                        stream(conn, &world, next, radius, &mut loaded)?;
                        center = next;
                    }
                }
                7 => {
                    let action = r.u8()?;
                    let pos = BlockPos::new(r.i32()?, i32::from(r.u8()?), r.i32()?);
                    let face = r.u8()?;
                    finish(&r)?;
                    if action > 5 || face > 5 {
                        return Err(Error::new("invalid 1.7 digging action"));
                    }
                    if action == 0 && loaded.contains(&pos.chunk()) && within_reach(&player, pos) {
                        changed(conn, &mut world, &loaded, pos, 0)?;
                    } else if action <= 2 {
                        send_block(conn, &world, pos)?;
                    } else if action == 3 || action == 4 {
                        inventory(conn)?;
                    }
                }
                8 => {
                    let anchor = BlockPos::new(r.i32()?, i32::from(r.u8()?), r.i32()?);
                    let face = r.u8()?;
                    let item = slot::read(&mut r)?;
                    let cursor = [r.u8()?, r.u8()?, r.u8()?];
                    finish(&r)?;
                    if anchor == BlockPos::new(-1, 255, -1) && face == 255 {
                        continue;
                    }
                    if face > 5 || cursor.iter().any(|&v| v > 16) {
                        return Err(Error::new("invalid 1.7 placement face/cursor"));
                    }
                    let (x, y, z) = [
                        (0, -1, 0),
                        (0, 1, 0),
                        (0, 0, -1),
                        (0, 0, 1),
                        (-1, 0, 0),
                        (1, 0, 0),
                    ][face as usize];
                    let pos = BlockPos::new(
                        anchor
                            .x
                            .checked_add(x)
                            .ok_or_else(|| Error::new("placement overflow"))?,
                        anchor.y + y,
                        anchor
                            .z
                            .checked_add(z)
                            .ok_or_else(|| Error::new("placement overflow"))?,
                    );
                    let matches = item.as_ref().is_some_and(|s| {
                        s.id == i16::from(HOTBAR[player.held as usize])
                            && s.damage == 0
                            && (1..=64).contains(&s.count)
                            && (s.tag == Tag::End
                                || matches!(&s.tag, Tag::Compound(v) if v.is_empty()))
                    });
                    if (0..256).contains(&pos.y)
                        && loaded.contains(&pos.chunk())
                        && within_reach(&player, anchor)
                        && within_reach(&player, pos)
                        && matches
                        && block_at(&world, anchor)? != 0
                        && block_at(&world, pos)? == 0
                        && !Aabb::block(pos).intersects(body(player.pose))
                    {
                        changed(conn, &mut world, &loaded, pos, HOTBAR[player.held as usize])?;
                    } else {
                        send_block(conn, &world, pos)?;
                        inventory(conn)?;
                    }
                }
                9 => {
                    let slot = r.i16()?;
                    finish(&r)?;
                    if !(0..9).contains(&slot) {
                        return Err(Error::new("invalid 1.7 hotbar slot"));
                    }
                    player.held = slot as u8;
                    held(conn, &player)?;
                }
                0x0a => {
                    r.i32()?;
                    r.u8()?;
                    finish(&r)?;
                }
                0x0b => {
                    let entity = r.i32()?;
                    let action = r.u8()?;
                    let boost = r.i32()?;
                    finish(&r)?;
                    if entity != 1 || !(1..=7).contains(&action) || !(0..=100).contains(&boost) {
                        return Err(Error::new("invalid 1.7 entity action"));
                    }
                }
                0x0d => {
                    r.u8()?;
                    finish(&r)?;
                }
                0x0e => {
                    let window = r.u8()?;
                    r.i16()?;
                    r.u8()?;
                    let action = r.i16()?;
                    r.u8()?;
                    slot::read(&mut r)?;
                    finish(&r)?;
                    let mut reply = packet(0x32);
                    reply.u8(window);
                    reply.i16(action);
                    reply.bool(false);
                    conn.send(&reply.data)?;
                    inventory(conn)?;
                }
                0x0f => {
                    r.u8()?;
                    r.i16()?;
                    boolean(&mut r)?;
                    finish(&r)?;
                }
                0x10 => {
                    let slot = r.i16()?;
                    slot::read(&mut r)?;
                    finish(&r)?;
                    if !(-1..=44).contains(&slot) {
                        return Err(Error::new("invalid creative slot"));
                    }
                    inventory(conn)?;
                }
                0x13 => {
                    let flags = r.u8()?;
                    let fly = r.f32()?;
                    let walk = r.f32()?;
                    finish(&r)?;
                    if flags & !15 != 0
                        || !fly.is_finite()
                        || !walk.is_finite()
                        || !(0.0..=1.0).contains(&fly)
                        || !(0.0..=1.0).contains(&walk)
                    {
                        return Err(Error::new("invalid 1.7 abilities"));
                    }
                    player.flying = flags & 2 != 0;
                    abilities(conn, &player)?;
                }
                0x15 => {
                    let locale = r.string(7)?;
                    let requested = r.i8()?;
                    let chat = r.u8()?;
                    boolean(&mut r)?;
                    let difficulty = r.u8()?;
                    boolean(&mut r)?;
                    finish(&r)?;
                    if locale.is_empty() || requested < 1 || chat > 2 || difficulty > 3 {
                        return Err(Error::new("invalid 1.7 client settings"));
                    }
                    let next = i32::from(requested).clamp(2, 4);
                    if next != radius {
                        stream(conn, &world, center, next, &mut loaded)?;
                        radius = next;
                    }
                }
                0x16 => {
                    let status = r.u8()?;
                    finish(&r)?;
                    if status > 2 {
                        return Err(Error::new("invalid 1.7 client status"));
                    }
                    if status == 1 {
                        let mut stats = packet(0x37);
                        stats.varint(0);
                        conn.send(&stats.data)?;
                    }
                }
                0x17 => {
                    r.string(20)?;
                    let length = r.i16()?;
                    if length < 0 {
                        return Err(Error::new("invalid 1.7 plugin length"));
                    }
                    r.bytes(length as usize)?;
                    finish(&r)?;
                }
                _ => {
                    return Err(Error::new(format!(
                        "unsupported restricted 1.7 action {id:#x}"
                    )));
                }
            }
        }
    })();
    player.save(&world, dir, version, &name)?;
    result
}
