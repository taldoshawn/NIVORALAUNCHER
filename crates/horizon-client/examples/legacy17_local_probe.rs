//! Exercise the real Session API against the native in-memory 1.7 engine.
//! No display, prepared game resources, account or real player data is used.
use horizon_client::net::{Event, Session};
use horizon_core::position::{BlockPos, ChunkPos};
use horizon_protocol::{
    legacy17::{Version, local_flat},
    legacy18::{chunk::Column, play::Packet},
    packets::{configuration::ClientInformation, play},
    text::Language,
};
use horizon_world::legacy17_flat::{Version as WorldVersion, World};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

fn wait(session: &Session, mut wanted: impl FnMut(i32, &[u8]) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        assert!(Instant::now() < deadline, "missing Session event");
        let mut done = false;
        for event in session.poll() {
            match event {
                Event::LegacyPacket { id, payload } => done |= wanted(id, &payload),
                Event::Disconnected { reason, .. } => panic!("unexpected disconnect: {reason}"),
                _ => {}
            }
        }
        if done {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn join(directory: &Path, version: Version, reopened: bool) -> Session {
    let session = Session::connect_local_legacy17(
        version,
        local_flat::open(directory.to_owned(), version).unwrap(),
        "Session17Probe",
        ClientInformation::new(2),
        Arc::new(Language::new()),
        directory.join("unused-resources"),
    );
    let mut columns = 0;
    let mut inventory = false;
    let mut position = false;
    let mut held = false;
    wait(&session, |id, bytes| {
        inventory |= id == 0x30;
        if let Ok(Packet::Position {
            x,
            y,
            z,
            yaw,
            pitch,
            ..
        }) = Packet::decode(id, bytes)
        {
            assert_eq!(
                (x, y, z, yaw, pitch),
                if reopened {
                    (1.5, 4., 0.5, 90., 15.)
                } else {
                    (0.5, 4., 0.5, 0., 0.)
                }
            );
            position = true;
        }
        if id == 9 {
            assert_eq!(bytes, if reopened { &[4] } else { &[0] });
            held = true;
        }
        if id == 0x21 {
            let column = Column::decode(bytes, true).unwrap();
            assert!(!column.is_unload());
            columns += 1;
        }
        columns == 25 && inventory && position && held
    });
    assert!(session.is_legacy());
    session
}

fn block(session: &Session, expected: u16) {
    wait(session, |id, bytes| {
        matches!(Packet::decode(id, bytes),
        Ok(Packet::BlockChange { x: 2, y: 4, z: 0, state }) if state == expected)
    });
}

fn check(directory: &Path, version: Version) {
    let session = join(directory, version, false);
    session.send(play::Serverbound::UseItemOn(play::UseItemOn {
        hand: 0,
        pos: [2, 3, 0],
        face: 1,
        cursor: [0.5, 1., 0.5],
        inside: false,
        world_border: false,
        sequence: 0,
    }));
    block(&session, 16);
    session.send(play::Serverbound::MovePlayerPosRot(
        play::MovePlayerPosRot {
            x: 1.5,
            y: 4.,
            z: 0.5,
            yaw: 90.,
            pitch: 15.,
            flags: 1,
        },
    ));
    session.send(play::Serverbound::SetCarriedItem(play::SetCarriedItem {
        slot: 4,
    }));
    wait(&session, |id, bytes| id == 9 && bytes == [4]);
    // Stay idle beyond the server's 15-second pending-keepalive deadline.
    // A Session that merely ignores heartbeats must fail the following action.
    std::thread::sleep(Duration::from_secs(19));
    session.send(play::Serverbound::SetCarriedItem(play::SetCarriedItem {
        slot: 4,
    }));
    wait(&session, |id, bytes| id == 9 && bytes == [4]);
    drop(session);
    let world_version = match version {
        Version::V1_7_2 => WorldVersion::V1_7_2,
        Version::V1_7_10 => WorldVersion::V1_7_10,
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    let world = loop {
        match World::open(directory, world_version) {
            Ok(world) => break world,
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            Err(error) => panic!("engine did not release its save: {error}"),
        }
    };
    assert_eq!(
        world
            .column(ChunkPos::new(0, 0))
            .unwrap()
            .block(BlockPos::new(2, 4, 0))
            .unwrap(),
        1
    );
    drop(world);
    let session = join(directory, version, true);
    session.send(play::Serverbound::PlayerAction(play::PlayerAction {
        action: 0,
        pos: [2, 4, 0],
        direction: 1,
        sequence: 1,
    }));
    block(&session, 0);
    drop(session);
}

fn main() {
    let directory = std::env::args_os()
        .nth(1)
        .expect("supply a new disposable directory");
    let directory = Path::new(&directory);
    std::fs::create_dir(directory).expect("directory must not already exist");
    for (name, version) in [("1.7.2", Version::V1_7_2), ("1.7.10", Version::V1_7_10)] {
        let mut title = horizon_client::title::Title::new(directory.join("saves"), directory);
        title.legacy18 = true;
        title.legacy17 = Some(version);
        // Main-menu singleplayer at the public 400x300 UI coordinates.
        assert_eq!(
            title.click(200., 133., (400., 300.)).0,
            horizon_client::title::Action::ClassicFlat17
        );
        assert!(!directory.join("saves").exists());
        check(&directory.join(name), version);
        println!(
            "{name}: native Session login, chunks, inventory, edit, movement, idle and reopen passed"
        );
    }
}
