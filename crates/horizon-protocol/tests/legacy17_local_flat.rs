use horizon_core::position::{BlockPos, ChunkPos};
use horizon_protocol::{
    buf::{Reader, Writer},
    connection::Connection,
    crypto::{offline_uuid, uuid_string},
    legacy17::{
        self, Version,
        actions::{self, Digging},
        chunk::Column,
        local_flat,
        play::{Packet, Pose},
        slot::{self, Slot},
    },
};
use horizon_world::{
    compression::Compression,
    legacy17_flat::{Version as WorldVersion, World},
    nbt::{self, Limits, Tag},
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

const NAME: &str = "Flat17Fixture";
const HOTBAR: [i16; 9] = [1, 3, 4, 5, 20, 45, 24, 22, 42];
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "nivora-local17-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn world_version(version: Version) -> WorldVersion {
    match version {
        Version::V1_7_2 => WorldVersion::V1_7_2,
        Version::V1_7_10 => WorldVersion::V1_7_10,
    }
}
fn item(id: i16) -> Slot {
    Slot {
        id,
        count: 64,
        damage: 0,
        tag: Tag::End,
    }
}
fn next(conn: &mut Connection) -> (i32, Vec<u8>) {
    let (id, bytes) = conn.recv().unwrap();
    if id == 0 {
        let value = Reader::new(&bytes).i32().unwrap();
        conn.send(&legacy17::play::keep_alive(value)).unwrap();
    }
    (id, bytes)
}
fn until(conn: &mut Connection, mut wanted: impl FnMut(i32, &[u8]) -> bool) -> Vec<u8> {
    for _ in 0..600 {
        let (id, bytes) = next(conn);
        if wanted(id, &bytes) {
            return bytes;
        }
        assert_ne!(
            id,
            0x40,
            "unexpected disconnect: {:?}",
            Packet::decode(id, &bytes)
        );
    }
    panic!("expected packet was not received")
}
fn inventory(bytes: &[u8]) {
    let mut r = Reader::new(bytes);
    assert_eq!(r.u8().unwrap(), 0);
    assert_eq!(r.i16().unwrap(), 45);
    for index in 0..45 {
        let value = slot::read(&mut r).unwrap();
        if index < 36 {
            assert!(value.is_none());
        } else {
            let value = value.unwrap();
            assert_eq!(
                (value.id, value.count, value.damage),
                (HOTBAR[index - 36], 64, 0)
            );
        }
    }
    assert_eq!(r.remaining(), 0);
}
fn clear_cursor(bytes: &[u8]) {
    let mut r = Reader::new(bytes);
    assert_eq!(r.i8().unwrap(), -1);
    assert_eq!(r.i16().unwrap(), -1);
    assert!(slot::read(&mut r).unwrap().is_none());
    assert_eq!(r.remaining(), 0);
}
fn connect(directory: &Path, version: Version) -> (Connection, Pose, u8) {
    let pipe = local_flat::open(directory.to_owned(), version).unwrap();
    let (mut conn, profile) = legacy17::login(
        version,
        Connection::local(pipe),
        "local",
        0,
        NAME,
        Duration::from_secs(5),
        |_| panic!("memory world must not request authentication"),
    )
    .unwrap();
    assert_eq!(profile.uuid, offline_uuid(NAME));
    conn.reader
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut pose = None;
    let mut held = None;
    let mut columns = 0;
    let mut joined = false;
    let mut noon = false;
    let mut inv = false;
    while columns < 25 {
        let (id, bytes) = next(&mut conn);
        match Packet::decode(id, &bytes).unwrap() {
            Packet::Join {
                gamemode,
                dimension,
                level_type,
                ..
            } => {
                assert_eq!((gamemode, dimension, level_type.as_str()), (1, 0, "flat"));
                joined = true;
            }
            Packet::Time {
                day_time, ticking, ..
            } => {
                assert_eq!(day_time, 6000);
                assert!(!ticking);
                noon = true;
            }
            Packet::Position {
                x,
                y,
                z,
                yaw,
                pitch,
                ..
            } => {
                pose = Some(Pose {
                    x,
                    y,
                    z,
                    yaw,
                    pitch,
                })
            }
            Packet::HeldSlot(value) => held = Some(value),
            Packet::Chunk(payload) => {
                let col = Column::decode(payload, true).unwrap();
                assert!(!col.is_unload());
                columns += 1;
            }
            Packet::Disconnect(reason) => panic!("{reason}"),
            _ => {}
        }
        if id == 0x30 {
            inventory(&bytes);
            inv = true;
        }
        if id == 0x2f {
            clear_cursor(&bytes);
        }
    }
    assert!(joined && noon && inv);
    (conn, pose.unwrap(), held.unwrap())
}
fn block(conn: &mut Connection, pos: BlockPos, expected: u16) {
    until(
        conn,
        |id, bytes| matches!(Packet::decode(id,bytes),Ok(Packet::BlockChange{x,y,z,state}) if (x,i32::from(y),z)==(pos.x,pos.y,pos.z) && state==expected),
    );
}
fn unlocked(directory: &Path, version: Version) -> World {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match World::open(directory, world_version(version)) {
            Ok(world) => return world,
            Err(e) if Instant::now() < deadline => {
                let _ = e;
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => panic!("world did not unlock: {e}"),
        }
    }
}
fn player_path(dir: &Path, version: Version) -> PathBuf {
    match version {
        Version::V1_7_2 => dir.join("players").join(format!("{NAME}.dat")),
        Version::V1_7_10 => dir
            .join("playerdata")
            .join(format!("{}.dat", uuid_string(offline_uuid(NAME)))),
    }
}

#[test]
fn both_native_protocols_place_break_move_and_reopen_legacy_player_saves() {
    for version in [Version::V1_7_2, Version::V1_7_10] {
        let dir = Temp::new();
        let (mut conn, pose, selected) = connect(&dir.0, version);
        assert_eq!((pose.x, pose.y, pose.z, selected), (0.5, 4., 0.5, 0));
        conn.send(&actions::place(2, 3, 0, 1, Some(&item(1)), [8, 16, 8]).unwrap())
            .unwrap();
        block(&mut conn, BlockPos::new(2, 4, 0), 16);
        until(&mut conn, |id, bytes| {
            id == 0x21
                && Column::decode(bytes, true).is_ok_and(|c| {
                    c.x == 0 && c.z == 0 && c.section(0).unwrap().state(2, 4, 0) == Some(16)
                })
        });
        conn.send(
            &Pose {
                x: 1.5,
                y: 4.,
                z: 0.5,
                yaw: 90.,
                pitch: 15.,
            }
            .encode(true)
            .unwrap(),
        )
        .unwrap();
        conn.send(&actions::held_slot(4).unwrap()).unwrap();
        until(&mut conn, |id, b| {
            matches!(Packet::decode(id, b), Ok(Packet::HeldSlot(4)))
        });
        conn.send(&actions::place(3, 3, 0, 1, Some(&item(20)), [8, 16, 8]).unwrap())
            .unwrap();
        block(&mut conn, BlockPos::new(3, 4, 0), 320);
        conn.send(&actions::digging(Digging::Start, 2, 4, 0, 1).unwrap())
            .unwrap();
        block(&mut conn, BlockPos::new(2, 4, 0), 0);
        let mut flight = Writer::new();
        flight.varint(0x13);
        flight.u8(15);
        flight.f32(0.05);
        flight.f32(0.1);
        conn.send(&flight.data).unwrap();
        until(&mut conn, |id, b| id == 0x39 && b[0] == 15);
        // No movement is needed to drive the server's heartbeat.
        until(&mut conn, |id, _| id == 0);
        conn.writer.shutdown();
        drop(conn);
        let world = unlocked(&dir.0, version);
        let column = world.column(ChunkPos::new(0, 0)).unwrap();
        assert_eq!(column.block(BlockPos::new(2, 4, 0)).unwrap(), 0);
        assert_eq!(column.block(BlockPos::new(3, 4, 0)).unwrap(), 20);
        drop(world);
        let saved = Compression::Gzip
            .decode(&fs::read(player_path(&dir.0, version)).unwrap(), 65536)
            .unwrap();
        let (root, used) = nbt::read_named_legacy(&saved, Limits::default()).unwrap();
        assert_eq!(used, saved.len());
        assert_eq!(
            root.tag.get("NivoraProtocol"),
            Some(&Tag::Int(version.protocol()))
        );
        assert_eq!(root.tag.get("SelectedItemSlot"), Some(&Tag::Int(4)));
        assert_eq!(
            root.tag.get("Pos"),
            Some(&Tag::List(vec![
                Tag::Double(1.5f64.to_bits()),
                Tag::Double(4f64.to_bits()),
                Tag::Double(0.5f64.to_bits())
            ]))
        );
        let (mut conn, pose, held) = connect(&dir.0, version);
        assert_eq!(
            (pose.x, pose.y, pose.z, pose.yaw, pose.pitch, held),
            (1.5, 4., 0.5, 90., 15., 4)
        );
        conn.send(&actions::digging(Digging::Start, 3, 4, 0, 1).unwrap())
            .unwrap();
        block(&mut conn, BlockPos::new(3, 4, 0), 0);
        conn.writer.shutdown();
        drop(conn);
        drop(unlocked(&dir.0, version));
    }
}

#[test]
fn reach_collision_and_unsupported_inventory_are_rejected_without_ghost_items() {
    let dir = Temp::new();
    let version = Version::V1_7_10;
    let (mut conn, _, _) = connect(&dir.0, version);
    for (x, z) in [(100, 100), (0, 0)] {
        conn.send(&actions::place(x, 3, z, 1, Some(&item(1)), [8, 16, 8]).unwrap())
            .unwrap();
        block(&mut conn, BlockPos::new(x, 4, z), 0);
        inventory(&until(&mut conn, |id, _| id == 0x30));
        clear_cursor(&until(&mut conn, |id, _| id == 0x2f));
    }
    conn.send(&actions::place(2, 3, 0, 1, Some(&item(1)), [8, 16, 8]).unwrap())
        .unwrap();
    block(&mut conn, BlockPos::new(2, 4, 0), 16);
    conn.send(
        &Pose {
            x: 2.5,
            y: 4.,
            z: 0.5,
            ..Pose::default()
        }
        .encode(true)
        .unwrap(),
    )
    .unwrap();
    let corrected = until(&mut conn, |id, _| id == 8);
    let Packet::Position { x, y, z, .. } = Packet::decode(8, &corrected).unwrap() else {
        panic!()
    };
    assert_eq!((x, y, z), (0.5, 4., 0.5));
    conn.send(&actions::creative_slot(36, Some(&item(10))).unwrap())
        .unwrap();
    inventory(&until(&mut conn, |id, _| id == 0x30));
    clear_cursor(&until(&mut conn, |id, _| id == 0x2f));
    for action in [Digging::DropOne, Digging::DropStack] {
        conn.send(&actions::digging(action, 0, 0, 0, 0).unwrap())
            .unwrap();
        inventory(&until(&mut conn, |id, _| id == 0x30));
        clear_cursor(&until(&mut conn, |id, _| id == 0x2f));
    }
    conn.send(&actions::place(3, 3, 0, 1, Some(&item(10)), [8, 16, 8]).unwrap())
        .unwrap();
    block(&mut conn, BlockPos::new(3, 4, 0), 0);
    inventory(&until(&mut conn, |id, _| id == 0x30));
    clear_cursor(&until(&mut conn, |id, _| id == 0x2f));
    let mut malformed = actions::place(3, 3, 0, 1, Some(&item(1)), [8, 16, 8]).unwrap();
    malformed.push(0);
    conn.send(&malformed).unwrap();
    until(&mut conn, |id, _| id == 0x40);
    conn.writer.shutdown();
    drop(conn);
    let world = unlocked(&dir.0, version);
    let column = world.column(ChunkPos::new(0, 0)).unwrap();
    assert_eq!(column.block(BlockPos::new(3, 4, 0)).unwrap(), 0);
    assert_eq!(column.block(BlockPos::new(2, 4, 0)).unwrap(), 1);
}

#[test]
fn view_distance_is_bounded_and_mismatched_protocol_does_not_enter_play() {
    let dir = Temp::new();
    let version = Version::V1_7_2;
    let (mut conn, _, _) = connect(&dir.0, version);
    conn.send(&actions::client_settings("pt_BR", 32, 0, true, 0, true).unwrap())
        .unwrap();
    conn.send(&actions::held_slot(1).unwrap()).unwrap();
    let mut count = 0;
    until(&mut conn, |id, bytes| {
        if id == 0x21 {
            let c = Column::decode(bytes, true).unwrap();
            assert!(!c.is_unload());
            assert!(c.x.abs() <= 4 && c.z.abs() <= 4);
            count += 1;
        }
        matches!(Packet::decode(id, bytes), Ok(Packet::HeldSlot(1)))
    });
    assert_eq!(count, 56);
    conn.send(&actions::client_settings("en_US", 1, 0, true, 0, true).unwrap())
        .unwrap();
    conn.send(&actions::held_slot(0).unwrap()).unwrap();
    let mut count = 0;
    until(&mut conn, |id, bytes| {
        if id == 0x21 {
            assert!(Column::decode(bytes, true).unwrap().is_unload());
            count += 1;
        }
        matches!(Packet::decode(id, bytes), Ok(Packet::HeldSlot(0)))
    });
    assert_eq!(count, 56);
    conn.writer.shutdown();
    drop(conn);
    drop(unlocked(&dir.0, version));
    let pipe = local_flat::open(dir.0.clone(), version).unwrap();
    let outcome = legacy17::login_outcome(
        Version::V1_7_10,
        Connection::local(pipe),
        "local",
        0,
        NAME,
        Duration::from_secs(5),
        |_| panic!(),
    )
    .unwrap();
    assert!(matches!(outcome, legacy17::LoginOutcome::Disconnected(_)));
    drop(unlocked(&dir.0, version));
}

#[test]
fn foreign_or_truncated_player_saves_and_unmarked_worlds_remain_untouched() {
    for version in [Version::V1_7_2, Version::V1_7_10] {
        let dir = Temp::new();
        let (conn, _, _) = connect(&dir.0, version);
        conn.writer.shutdown();
        drop(conn);
        drop(unlocked(&dir.0, version));
        let path = player_path(&dir.0, version);
        let original = fs::read(&path).unwrap();
        let foreign = Compression::Gzip
            .encode(
                &nbt::write_named_legacy(
                    &nbt::NamedTag {
                        name: Default::default(),
                        tag: Tag::compound([("foreign", Tag::Int(1))]),
                    },
                    Limits::default(),
                )
                .unwrap(),
            )
            .unwrap();
        for bytes in [foreign, original[..original.len() - 1].to_vec()] {
            fs::write(&path, &bytes).unwrap();
            let pipe = local_flat::open(dir.0.clone(), version).unwrap();
            let outcome = legacy17::login_outcome(
                version,
                Connection::local(pipe),
                "local",
                0,
                NAME,
                Duration::from_secs(5),
                |_| panic!(),
            )
            .unwrap();
            assert!(matches!(outcome, legacy17::LoginOutcome::Disconnected(_)));
            drop(unlocked(&dir.0, version));
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
    }
    let foreign = Temp::new();
    fs::write(foreign.0.join("level.dat"), b"original foreign save").unwrap();
    assert!(local_flat::open(foreign.0.clone(), Version::V1_7_10).is_err());
    assert_eq!(
        fs::read(foreign.0.join("level.dat")).unwrap(),
        b"original foreign save"
    );
}
