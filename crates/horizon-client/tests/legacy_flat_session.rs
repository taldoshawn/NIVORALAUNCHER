use horizon_client::net::{Event, Session};
use horizon_core::position::{BlockPos, ChunkPos};
use horizon_protocol::{
    legacy17::{Version, local_flat},
    legacy18::{chunk::Column, play::Packet},
    packets::{configuration::ClientInformation, play},
};
use horizon_world::legacy17_flat::{Version as WorldVersion, World};
use std::{
    path::PathBuf,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

#[test]
fn classic_17_sessions_load_a_world_and_persist_client_actions() {
    let home = std::env::temp_dir().join(format!("nivora-flat-session-{}", std::process::id()));
    std::fs::create_dir(&home).unwrap();
    for (version, world_version) in [
        (Version::V1_7_2, WorldVersion::V1_7_2),
        (Version::V1_7_10, WorldVersion::V1_7_10),
    ] {
        let directory = home.join(version.release());
        let pipe = local_flat::open(directory.clone(), version).unwrap();
        let mut session = Session::connect_local_legacy17(
            pipe,
            version,
            "FlatProbe",
            ClientInformation::new(2),
            Arc::default(),
            PathBuf::new(),
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        let (mut joined, mut center, mut broken) = (false, false, false);
        while !broken {
            assert!(Instant::now() < deadline, "timed out for {version:?}");
            for event in session.poll() {
                match event {
                    Event::Joined { name, .. } => {
                        assert_eq!(name, "FlatProbe");
                        joined = true;
                    }
                    Event::LegacyPacket { id, payload } => {
                        match Packet::decode(id, &payload).unwrap() {
                            Packet::Chunk(bytes) if !center => {
                                let column = Column::decode(bytes, true).unwrap();
                                if (column.x, column.z) == (0, 0) {
                                    assert!(joined);
                                    center = true;
                                    session.send(play::Serverbound::PlayerAction(
                                        play::PlayerAction {
                                            action: 0,
                                            pos: [2, 3, 0],
                                            direction: 1,
                                            sequence: 0,
                                        },
                                    ));
                                }
                            }
                            Packet::BlockChange {
                                x: 2,
                                y: 3,
                                z: 0,
                                state: 0,
                            } => broken = true,
                            _ => {}
                        }
                    }
                    Event::Disconnected { reason, .. } => panic!("{version:?}: {reason}"),
                    _ => {}
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        session.disconnect();
        let deadline = Instant::now() + Duration::from_secs(5);
        let world = loop {
            match World::open(&directory, world_version) {
                Ok(world) => break world,
                Err(_) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("could not reopen {version:?}: {error}"),
            }
        };
        let column = world.column(ChunkPos::new(0, 0)).unwrap();
        assert_eq!(column.block(BlockPos::new(2, 3, 0)).unwrap(), 0);
        assert_eq!(column.block(BlockPos::new(3, 3, 0)).unwrap(), 2);
    }
    std::fs::remove_dir_all(home).unwrap();
}
