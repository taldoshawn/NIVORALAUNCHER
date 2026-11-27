//! In-process creative classic-flat sandbox. Its region files are isolated from
//! modern worlds. Only the local player owns the memory channel.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    connection::{Connection, Pipe},
    crypto::{offline_uuid, uuid_string},
};
use horizon_core::position::{BlockPos, ChunkPos};
use horizon_world::{
    compression::Compression,
    nbt::{Limits, Tag},
    numeric_chunk::NumericChunk,
    region::{ChunkRecord, RegionFile, RegionWriter},
};
use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
    time::Instant,
};

const HOTBAR: [u8; 9] = [1, 3, 4, 5, 20, 45, 24, 22, 42];

pub fn open(directory: PathBuf) -> Result<Pipe> {
    fs::create_dir_all(directory.join("region"))?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(directory.join("session.lock"))?;
    lock.try_lock()
        .map_err(|e| Error::new(format!("Could not open classic flat world: {e}")))?;
    let (client, server) = Pipe::pair();
    std::thread::Builder::new()
        .name("nivora-classic-flat".into())
        .spawn(move || {
            let _lock = lock;
            let mut conn = Connection::local(server);
            if let Err(e) = play(&mut conn, &directory) {
                let mut kick = packet(0x40);
                kick.string(&serde_json::json!({"text": e.to_string()}).to_string());
                let _ = conn.send(&kick.data);
            }
        })?;
    Ok(client)
}

fn packet(id: i32) -> Writer {
    let mut w = Writer::new();
    w.varint(id);
    w
}
fn region(dir: &Path, p: ChunkPos) -> PathBuf {
    let r = p.region();
    dir.join("region").join(format!("r.{}.{}.mca", r.x, r.z))
}
fn open_region(path: &Path, writable: bool) -> Result<Option<RegionFile>> {
    match RegionFile::open_strict(path, writable) {
        Ok(file) => Ok(Some(file)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn load(file: Option<&mut RegionFile>, p: ChunkPos) -> Result<NumericChunk> {
    if let Some(file) = file
        && let Some(record) = file.read(p)?
    {
        let root = record.nbt(Limits::default())?;
        let level = root.tag.get("Level");
        if level.and_then(|l| l.get("xPos")) != Some(&Tag::Int(p.x))
            || level.and_then(|l| l.get("zPos")) != Some(&Tag::Int(p.z))
        {
            return Err(Error::new(
                "Classic-flat chunk coordinates do not match its region slot",
            ));
        }
        return Ok(NumericChunk::new(root)?);
    }
    Ok(NumericChunk::classic_flat_189(p))
}
fn section_bytes<'a>(tag: &'a Tag, name: &str, len: usize) -> Result<&'a [u8]> {
    match tag.get(name) {
        Some(Tag::ByteArray(bytes)) if bytes.len() == len => Ok(bytes),
        _ => Err(Error::new(format!("Invalid saved chunk field {name}"))),
    }
}
fn send_column(conn: &mut Connection, dir: &Path, p: ChunkPos) -> Result<()> {
    let root = load(open_region(&region(dir, p), false)?.as_mut(), p)?.into_nbt();
    let level = root.tag.get("Level").unwrap();
    let Some(Tag::List(sections)) = level.get("Sections") else {
        unreachable!()
    };
    let mut ordered = [None; 16];
    let mut mask = 0u16;
    for s in sections {
        let Some(Tag::Byte(y)) = s.get("Y") else {
            unreachable!()
        };
        ordered[*y as usize] = Some(s);
        mask |= 1 << *y;
    }
    let mut bytes = Vec::with_capacity(sections.len() * 12288 + 256);
    for s in ordered.iter().flatten() {
        let blocks = section_bytes(s, "Blocks", 4096)?;
        let meta = section_bytes(s, "Data", 2048)?;
        let add = if s.get("Add").is_some() {
            Some(section_bytes(s, "Add", 2048)?)
        } else {
            None
        };
        for (i, id) in blocks.iter().enumerate() {
            let shift = (i % 2) * 4;
            let high = add.map_or(0, |a| (a[i / 2] >> shift) & 15);
            let state = (u16::from(high) << 12)
                | (u16::from(*id) << 4)
                | u16::from((meta[i / 2] >> shift) & 15);
            bytes.extend_from_slice(&state.to_le_bytes());
        }
    }
    for field in ["BlockLight", "SkyLight"] {
        for s in ordered.iter().flatten() {
            bytes.extend_from_slice(section_bytes(s, field, 2048)?);
        }
    }
    bytes.extend_from_slice(section_bytes(level, "Biomes", 256)?);
    let mut w = packet(0x21);
    w.i32(p.x);
    w.i32(p.z);
    w.bool(true);
    w.u16(mask);
    w.byte_array(&bytes);
    conn.send(&w.data)
}
fn stream(
    conn: &mut Connection,
    dir: &Path,
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
    for p in loaded.difference(&next) {
        let mut w = packet(0x21);
        w.i32(p.x);
        w.i32(p.z);
        w.bool(true);
        w.u16(0);
        w.byte_array(&[]);
        conn.send(&w.data)?;
    }
    let mut missing: Vec<_> = next.difference(loaded).copied().collect();
    missing.sort_unstable_by_key(|p| (p.x - center.x).pow(2) + (p.z - center.z).pow(2));
    for p in missing {
        send_column(conn, dir, p)?;
    }
    *loaded = next;
    Ok(())
}
fn update(conn: &mut Connection, dir: &Path, pos: BlockPos, id: u8) -> Result<()> {
    let p = pos.chunk();
    let path = region(dir, p);
    let mut file = open_region(&path, true)?;
    let mut chunk = load(file.as_mut(), p)?;
    if chunk.block(pos)? == (u16::from(id), 0) {
        return Ok(());
    }
    chunk.set_block(pos, u16::from(id), 0)?;
    chunk.update_classic_flat_skylight()?;
    let record = ChunkRecord::from_nbt(&chunk.into_nbt(), Compression::Zlib, 0, Limits::default())?;
    if let Some(mut file) = file {
        file.write(p, &record)?;
        file.sync()?;
    } else {
        let mut writer = RegionWriter::create(path)?;
        writer.write(p, &record)?;
        writer.finish()?;
    }
    let mut w = packet(0x23);
    w.i64(super::actions::block_position(pos.x, pos.y, pos.z)? as i64);
    w.varint(i32::from(id) << 4);
    conn.send(&w.data)?;
    send_column(conn, dir, p)
}
fn unpack(r: &mut Reader<'_>) -> Result<BlockPos> {
    let v = r.i64()?;
    Ok(BlockPos::new(
        (v >> 38) as i32,
        ((v >> 26) & 4095) as i32,
        ((v << 38) >> 38) as i32,
    ))
}

fn valid_pose(p: &[f64; 5]) -> bool {
    p.iter().all(|n| n.is_finite())
        && p[0].abs() < 30_000_000.
        && p[2].abs() < 30_000_000.
        && (-64.0..=4096.0).contains(&p[1])
        && p[4].abs() <= 90.0
}

#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PlayerSave {
    version: u8,
    pose: [f64; 5],
    held: u8,
}

fn read_player(dir: &Path) -> Result<PlayerSave> {
    let default = PlayerSave {
        version: 1,
        pose: [0.5, 4.0, 0.5, 0., 0.],
        held: 0,
    };
    let bytes = match fs::read(dir.join("player.json")) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(default),
        Err(e) => return Err(e.into()),
    };
    // Existing worlds stored just the position array.
    let player = serde_json::from_slice::<PlayerSave>(&bytes)
        .or_else(|_| {
            serde_json::from_slice::<[f64; 5]>(&bytes).map(|pose| PlayerSave { pose, ..default })
        })
        .map_err(|_| Error::new("Invalid saved classic-flat player state"))?;
    if player.version != 1 || !valid_pose(&player.pose) || player.held >= 9 {
        return Err(Error::new(
            "Invalid or unsupported classic-flat player state",
        ));
    }
    Ok(player)
}

fn save_json(dir: &Path, name: &str, value: &impl serde::Serialize) -> Result<()> {
    use std::io::Write;
    let tmp = dir.join(format!("{name}.tmp"));
    let mut f = fs::File::create(&tmp)?;
    serde_json::to_writer(&mut f, value).map_err(|e| Error::new(e.to_string()))?;
    f.flush()?;
    f.sync_all()?;
    drop(f);
    fs::rename(tmp, dir.join(name))?;
    Ok(())
}

// This world has a fixed initial noon offset; only its elapsed ticks need saving.
fn read_age(dir: &Path) -> Result<i64> {
    match fs::read(dir.join("world-age.json")) {
        Ok(bytes) => serde_json::from_slice::<i64>(&bytes)
            .ok()
            .filter(|age| (0..=i64::MAX - 6000).contains(age))
            .ok_or_else(|| Error::new("Invalid saved classic-flat world age")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(e) => Err(e.into()),
    }
}

fn send_time(conn: &mut Connection, age: i64) -> Result<()> {
    let mut w = packet(3);
    w.i64(age);
    w.i64(age + 6000);
    conn.send(&w.data)
}

fn teleport(conn: &mut Connection, pose: [f64; 5]) -> Result<()> {
    let mut w = packet(8);
    for coord in &pose[..3] {
        w.f64(*coord);
    }
    w.f32(pose[3] as f32);
    w.f32(pose[4] as f32);
    w.u8(0);
    conn.send(&w.data)
}

fn play(conn: &mut Connection, dir: &Path) -> Result<()> {
    let (id, body) = conn.recv()?;
    let mut r = Reader::new(&body);
    if id != 0 || r.varint()? != 47 {
        return Err(Error::new("Expected protocol 47 handshake"));
    }
    r.string(255)?;
    r.u16()?;
    if r.varint()? != 2 {
        return Err(Error::new("Expected login intention"));
    }
    let (id, body) = conn.recv()?;
    let name = Reader::new(&body).string(16)?;
    if id != 0 {
        return Err(Error::new("Expected login start"));
    }
    let mut w = packet(2);
    w.string(&uuid_string(offline_uuid(&name)));
    w.string(&name);
    conn.send(&w.data)?;
    let mut w = packet(1);
    w.i32(1);
    w.u8(1);
    w.i8(0);
    w.u8(0);
    w.u8(1);
    w.string("flat");
    w.bool(false);
    conn.send(&w.data)?;
    let mut w = packet(0x39);
    w.u8(13);
    w.f32(0.05);
    w.f32(0.1);
    conn.send(&w.data)?;
    let initial_age = read_age(dir)?;
    send_time(conn, initial_age)?;
    let mut player = read_player(dir)?;
    let mut saved_player = player;
    let mut pose = player.pose;
    teleport(conn, pose)?;
    for (i, id) in HOTBAR.iter().enumerate() {
        let mut w = packet(0x2f);
        w.u8(0);
        w.i16(36 + i as i16);
        w.i16(i16::from(*id));
        w.i8(64);
        w.i16(0);
        w.u8(0);
        conn.send(&w.data)?;
    }
    let mut held = usize::from(player.held);
    let mut w = packet(9);
    w.u8(player.held);
    conn.send(&w.data)?;
    let mut center = BlockPos::new(pose[0].floor() as i32, 0, pose[2].floor() as i32).chunk();
    let mut loaded = HashSet::new();
    let mut radius = 2;
    stream(conn, dir, center, radius, &mut loaded)?;
    let started = Instant::now();
    let age_now = || {
        initial_age
            .saturating_add((started.elapsed().as_millis() / 50) as i64)
            .min(i64::MAX - 6000)
    };
    let mut last_sync = initial_age;
    let mut saved_age = initial_age;
    let mut keep_alive = started;
    let session = (|| -> Result<()> {
        loop {
            let (id, body) = conn.recv()?;
            let mut r = Reader::new(&body);
            match id {
                4..=6 => {
                    let mut moved = pose;
                    if id != 5 {
                        for c in &mut moved[..3] {
                            *c = r.f64()?;
                        }
                    }
                    if id != 4 {
                        moved[3] = f64::from(r.f32()?);
                        moved[4] = f64::from(r.f32()?);
                    }
                    if moved[1] < -32.0 {
                        moved = [0.5, 6.0, 0.5, 0., 0.];
                        teleport(conn, moved)?;
                    }
                    if valid_pose(&moved) {
                        pose = moved;
                        let next = BlockPos::new(pose[0].floor() as i32, 0, pose[2].floor() as i32)
                            .chunk();
                        if next != center {
                            stream(conn, dir, next, radius, &mut loaded)?;
                            center = next;
                        }
                    }
                }
                0x15 => {
                    r.string(7)?;
                    // Bound this in-memory preview's pending columns on small phones.
                    let requested = i32::from(r.i8()?).clamp(2, 8);
                    if requested != radius {
                        radius = requested;
                        stream(conn, dir, center, radius, &mut loaded)?;
                    }
                }
                7 => {
                    let action = r.u8()?;
                    let p = unpack(&mut r)?;
                    if action == 0 && p.y < 256 && loaded.contains(&p.chunk()) {
                        update(conn, dir, p, 0)?;
                    }
                }
                8 => {
                    let p = unpack(&mut r)?;
                    let face = r.u8()?;
                    if let Some(&(x, y, z)) = [
                        (0, -1, 0),
                        (0, 1, 0),
                        (0, 0, -1),
                        (0, 0, 1),
                        (-1, 0, 0),
                        (1, 0, 0),
                    ]
                    .get(face as usize)
                    {
                        let pos = p.offset(x, y, z);
                        if (0..256).contains(&pos.y) && loaded.contains(&pos.chunk()) {
                            update(conn, dir, pos, HOTBAR[held])?;
                        }
                    }
                }
                9 => {
                    let slot = r.i16()?;
                    if (0..9).contains(&slot) {
                        held = slot as usize;
                    }
                }
                _ => {}
            }
            player.pose = pose;
            player.held = held as u8;
            let age = age_now();
            if age - last_sync >= 20 {
                send_time(conn, age)?;
                last_sync = age;
            }
            if keep_alive.elapsed().as_secs() >= 10 {
                if saved_player != player {
                    save_json(dir, "player.json", &player)?;
                    saved_player = player;
                }
                if age != saved_age {
                    save_json(dir, "world-age.json", &age)?;
                    saved_age = age;
                }
                let mut w = packet(0);
                w.varint(1);
                conn.send(&w.data)?;
                keep_alive = Instant::now();
            }
        }
    })();
    player.pose = pose;
    player.held = held as u8;
    if saved_player != player {
        save_json(dir, "player.json", &player)?;
    }
    let age = age_now();
    if age != saved_age {
        save_json(dir, "world-age.json", &age)?;
    }
    session
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::legacy18::{chunk::Column, login, play::Packet};
    use std::time::Duration;

    #[test]
    fn old_player_positions_migrate_and_invalid_saves_are_preserved() {
        let mut suffix = [0; 8];
        getrandom::fill(&mut suffix).unwrap();
        let dir =
            std::env::temp_dir().join(format!("nivora-player-{}", u64::from_le_bytes(suffix)));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("player.json");
        fs::write(&path, b"[12.5,4,-20.5,180,45]").unwrap();
        let mut player = read_player(&dir).unwrap();
        assert_eq!(player.pose, [12.5, 4., -20.5, 180., 45.]);
        assert_eq!(player.held, 0);
        player.held = 8;
        save_json(&dir, "player.json", &player).unwrap();
        assert_eq!(read_player(&dir).unwrap().held, 8);
        assert_eq!(read_player(&dir).unwrap().pose, player.pose);
        for invalid in [
            r#"{"version":2,"pose":[0,4,0,0,0],"held":0}"#,
            r#"{"version":1,"pose":[0,4,0,0,0],"held":9}"#,
            "[0,4,0,0,91]",
            "[0,4",
        ] {
            fs::write(&path, invalid).unwrap();
            assert!(read_player(&dir).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), invalid);
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn damaged_region_is_not_treated_as_empty_or_modified() {
        let mut suffix = [0; 8];
        getrandom::fill(&mut suffix).unwrap();
        let dir =
            std::env::temp_dir().join(format!("nivora-damaged-{}", u64::from_le_bytes(suffix)));
        fs::create_dir_all(dir.join("region")).unwrap();
        let p = ChunkPos::new(0, 0);
        let path = region(&dir, p);
        // A neighboring chunk points past the end of the file. The generic
        // reader tolerates this, but touching a different chunk must not erase it.
        let mut bytes = vec![0; 8192];
        bytes[4..8].copy_from_slice(&0x00000301u32.to_be_bytes());
        fs::write(&path, &bytes).unwrap();
        let (server, _client) = Pipe::pair();
        let mut conn = Connection::local(server);
        assert!(update(&mut conn, &dir, BlockPos::new(0, 4, 0), 1).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);

        // Two chunks claiming the same sector must also fail closed.
        bytes.resize(12288, 0);
        bytes[..4].copy_from_slice(&0x00000201u32.to_be_bytes());
        bytes[4..8].copy_from_slice(&0x00000201u32.to_be_bytes());
        fs::write(&path, &bytes).unwrap();
        assert!(update(&mut conn, &dir, BlockPos::new(0, 4, 0), 1).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::remove_file(&path).unwrap();
        let wrong_chunk = NumericChunk::classic_flat_189(ChunkPos::new(1, 0));
        let record = ChunkRecord::from_nbt(
            &wrong_chunk.into_nbt(),
            Compression::Zlib,
            0,
            Limits::default(),
        )
        .unwrap();
        let mut writer = RegionWriter::create(&path).unwrap();
        writer.write(p, &record).unwrap();
        writer.finish().unwrap();
        let before = fs::read(&path).unwrap();
        assert!(update(&mut conn, &dir, BlockPos::new(0, 4, 0), 1).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn play_edit_walk_disconnect_and_reopen_through_the_memory_protocol() {
        let mut suffix = [0; 8];
        getrandom::fill(&mut suffix).unwrap();
        let dir =
            std::env::temp_dir().join(format!("nivora-classic-{}", u64::from_le_bytes(suffix)));
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(read_age(&dir).unwrap(), 0);
        for bad_age in ["-1", "9223372036854775807", "broken"] {
            fs::write(dir.join("world-age.json"), bad_age).unwrap();
            assert!(read_age(&dir).is_err());
            assert_eq!(
                fs::read_to_string(dir.join("world-age.json")).unwrap(),
                bad_age
            );
        }
        fs::write(dir.join("world-age.json"), "17999").unwrap();
        let connect = || {
            let pipe = open(dir.clone()).unwrap();
            let (c, _) = login(
                Connection::local(pipe),
                "local",
                0,
                "Alex",
                Duration::from_secs(3),
                |_| panic!("local authentication request"),
            )
            .unwrap();
            c.reader
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            c
        };
        let mut c = connect();
        assert!(open(dir.clone()).is_err());
        let mut columns = 0;
        while columns < 25 {
            let (id, bytes) = c.recv().unwrap();
            if id == 0x21 {
                let col = Column::decode(&bytes, true).unwrap();
                assert_eq!(col.section(0).unwrap().state(1, 3, 1), Some(2 << 4));
                columns += 1;
            }
        }
        // One complete tick before leaving, including an idle period.
        std::thread::sleep(Duration::from_millis(55));
        let mut select = packet(9);
        select.i16(2);
        c.send(&select.data).unwrap();
        let mut pos = packet(4);
        pos.f64(16.5);
        pos.f64(4.0);
        pos.f64(0.5);
        pos.bool(true);
        c.send(&pos.data).unwrap();
        let mut w = packet(8);
        w.i64(super::super::actions::block_position(16, 3, 0).unwrap() as i64);
        w.u8(1);
        w.i16(-1);
        w.bytes(&[8, 15, 8]);
        c.send(&w.data).unwrap();
        loop {
            let (id, b) = c.recv().unwrap();
            if id == 0x23 {
                assert_eq!(
                    Packet::decode(id, &b).unwrap(),
                    Packet::BlockChange {
                        x: 16,
                        y: 4,
                        z: 0,
                        state: 64
                    }
                );
                break;
            }
        }
        drop(c);
        let until = Instant::now() + Duration::from_secs(3);
        let lock = fs::OpenOptions::new()
            .write(true)
            .open(dir.join("session.lock"))
            .unwrap();
        while lock.try_lock().is_err() {
            assert!(
                Instant::now() < until,
                "world did not close after disconnect"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        drop(lock);
        let mut c = connect();
        let age = read_age(&dir).unwrap();
        assert!(age >= 18000);
        let mut restored_clock = false;
        let mut restored = false;
        let mut restored_slot = false;
        loop {
            let (id, b) = c.recv().unwrap();
            if id == 3 {
                let mut r = Reader::new(&b);
                assert_eq!(r.i64().unwrap(), age);
                assert_eq!(r.i64().unwrap(), 6000 + age);
                restored_clock = true;
            }
            if id == 9 {
                assert_eq!(Packet::decode(id, &b).unwrap(), Packet::HeldSlot(2));
                restored_slot = true;
            }
            if id == 8 {
                let mut r = Reader::new(&b);
                assert_eq!(r.f64().unwrap(), 16.5);
                restored = true;
            }
            if id == 0x21 {
                let col = Column::decode(&b, true).unwrap();
                if (col.x, col.z) == (1, 0) {
                    assert_eq!(col.section(0).unwrap().state(0, 4, 0), Some(64));
                    break;
                }
            }
        }
        assert!(restored);
        assert!(restored_slot);
        assert!(restored_clock);
        drop(c);
        fs::remove_dir_all(dir).unwrap();
    }
}
