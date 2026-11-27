//! Development-only probe against an isolated vanilla 1.7 reference server.
//! Uses the same incoming/action adapters as the native client. It does not
//! render frames or connect to public servers.
use horizon_protocol::{
    buf::Reader,
    connection::Connection,
    legacy17::{self, Version},
    legacy18::{self, chunk::Column, play::Packet},
    packets::play::{self as modern, Serverbound},
};
use std::{
    net::{SocketAddr, TcpStream},
    time::{Duration, Instant},
};

const NAME: &str = "Legacy17Probe";
type TestResult<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Default)]
struct Evidence {
    joined: bool,
    positioned: bool,
    columns: usize,
    states: usize,
    keepalives: usize,
    anchor_seen: bool,
    empty_target_seen: bool,
    metadata_seen: bool,
    fixture_position: bool,
    placement: bool,
    broken: bool,
}

fn send(
    connection: &mut Connection,
    outbound: &mut legacy18::bridge::Outbound,
    packet: Serverbound,
) -> TestResult<()> {
    let encoded = outbound.encode(&packet)?.ok_or("action was not encoded")?;
    connection.send(&legacy17::bridge::outgoing(&encoded, outbound.entity)?)?;
    Ok(())
}

fn command(
    connection: &mut Connection,
    outbound: &mut legacy18::bridge::Outbound,
    command: &str,
) -> TestResult<()> {
    send(
        connection,
        outbound,
        Serverbound::ChatCommand(modern::ChatCommand {
            command: command.to_owned(),
        }),
    )
}

fn inspect_column(column: &Column<'_>, evidence: &mut Evidence) -> TestResult<()> {
    if column.is_unload() {
        return Ok(());
    }
    evidence.columns += 1;
    for sy in 0..16 {
        if let Some(section) = column.section(sy) {
            evidence.states += section.states().count();
        }
    }
    if (column.x, column.z) == (0, 0) {
        let section = column.section(4).ok_or("fixture section missing")?;
        // Independent values supplied by /setblock in the reference server.
        evidence.anchor_seen |= section.state(0, 1, 3) == Some(1 << 4);
        evidence.empty_target_seen |= section.state(0, 1, 2) == Some(0);
        evidence.metadata_seen |= section.state(6, 1, 0) == Some((35 << 4) | 14);
    }
    Ok(())
}

fn main() -> TestResult<()> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let (address, version) = match arguments.as_slice() {
        [address] => (address, Version::V1_7_10),
        [address, flag, version] if flag == "--version" => (
            address,
            match version.as_str() {
                "1.7.2" => Version::V1_7_2,
                "1.7.10" => Version::V1_7_10,
                _ => return Err("the reference probe supports only 1.7.2 and 1.7.10".into()),
            },
        ),
        _ => {
            return Err(
                "usage: legacy17_server_probe 127.0.0.1:PORT [--version 1.7.2|1.7.10]".into(),
            );
        }
    };
    let address: SocketAddr = address.parse()?;
    if !address.ip().is_loopback() {
        return Err("the probe only permits loopback addresses".into());
    }
    let stream = TcpStream::connect_timeout(&address, Duration::from_secs(10))?;
    let (mut connection, profile) = legacy17::login(
        version,
        Connection::from_stream(stream)?,
        "localhost",
        address.port(),
        NAME,
        Duration::from_secs(10),
        |_| {
            Err(horizon_protocol::Error::new(
                "this isolated fixture uses offline authentication",
            ))
        },
    )?;
    connection
        .reader
        .set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut incoming = legacy17::bridge::Incoming::new(version);
    let mut outbound = legacy18::bridge::Outbound::default();
    let mut evidence = Evidence::default();
    let mut pose = legacy18::play::Pose::default();
    let mut skylight = true;
    let mut prepared = false;
    let mut placed = false;
    let mut moved_at = None;
    let mut keepalives_at_move = 0;
    let deadline = Instant::now() + Duration::from_secs(35);
    let place = Serverbound::UseItemOn(modern::UseItemOn {
        hand: 0,
        pos: [0, 65, 3],
        face: 2,
        cursor: [0.5, 0.5, 0.0],
        inside: false,
        world_border: false,
        sequence: 0,
    });
    while Instant::now() < deadline {
        let (wire_id, payload) = connection.recv()?;
        let translated = incoming.translate(wire_id, &payload).map_err(|error| {
            format!(
                "incoming packet {wire_id:#x} ({} bytes): {error}",
                payload.len()
            )
        })?;
        for (id, bytes) in translated {
            if let Some(reply) = outbound.observe(id, &bytes)? {
                connection.send(&legacy17::bridge::outgoing(&reply, outbound.entity)?)?;
            }
            let packet = Packet::decode(id, &bytes)?;
            match packet {
                Packet::KeepAlive(value) => {
                    connection.send(&legacy17::bridge::outgoing(
                        &legacy18::play::keep_alive(value),
                        outbound.entity,
                    )?)?;
                    evidence.keepalives += 1;
                }
                Packet::Join {
                    entity,
                    dimension,
                    gamemode,
                    ..
                } => {
                    if gamemode & 7 != 1 {
                        return Err("fixture must put the player in creative mode".into());
                    }
                    outbound.entity = entity;
                    skylight = dimension == 0;
                    evidence.joined = true;
                    connection.send(&legacy17::actions::client_settings(
                        "en_US", 3, 0, true, 2, true,
                    )?)?;
                }
                Packet::Position { .. } => {
                    pose = pose.corrected(&packet)?;
                    evidence.positioned = true;
                    evidence.fixture_position = (pose.x - 0.5).abs() < 1e-6
                        && (pose.y - 64.0).abs() < 1e-6
                        && (pose.z - 0.5).abs() < 1e-6;
                    // The adapter must insert 1.7's separate stance Y.
                    connection.send(&legacy17::bridge::outgoing(
                        &pose.encode(false)?,
                        outbound.entity,
                    )?)?;
                }
                Packet::Chunk(body) => {
                    inspect_column(&Column::decode(body, skylight)?, &mut evidence)?
                }
                Packet::BlockChange { x, y, z, state } if (x, y, z) == (0, 65, 2) => {
                    if placed && state == 16 && !evidence.placement {
                        evidence.placement = true;
                        send(
                            &mut connection,
                            &mut outbound,
                            Serverbound::PlayerAction(modern::PlayerAction {
                                action: 0,
                                pos: [0, 65, 2],
                                direction: 2,
                                sequence: 0,
                            }),
                        )?;
                    } else if evidence.placement && state == 0 && !evidence.broken {
                        evidence.broken = true;
                        send(
                            &mut connection,
                            &mut outbound,
                            Serverbound::MovePlayerPosRot(modern::MovePlayerPosRot {
                                x: 1.5,
                                y: 64.0,
                                z: 0.5,
                                yaw: 0.0,
                                pitch: 0.0,
                                flags: 1,
                            }),
                        )?;
                        moved_at = Some(Instant::now());
                        keepalives_at_move = evidence.keepalives;
                    }
                }
                Packet::Disconnect(reason) => {
                    return Err(format!("reference server disconnected: {reason}").into());
                }
                _ => {}
            }
        }
        if evidence.joined && evidence.positioned && evidence.columns >= 4 && !prepared {
            command(
                &mut connection,
                &mut outbound,
                "tp Legacy17Probe 0.5 64.0 0.5",
            )?;
            command(
                &mut connection,
                &mut outbound,
                "give Legacy17Probe stone 64",
            )?;
            send(
                &mut connection,
                &mut outbound,
                Serverbound::SetCarriedItem(modern::SetCarriedItem { slot: 0 }),
            )?;
            prepared = true;
        }
        if prepared
            && evidence.fixture_position
            && evidence.anchor_seen
            && evidence.empty_target_seen
            && evidence.metadata_seen
            && !placed
        {
            let encoded = outbound
                .encode(&place)?
                .ok_or("placement was not encoded")?;
            // Wait for the actual server inventory update. These internal 47
            // bytes only reach the adapter, never the 1.7 socket directly.
            let held = legacy18::slot::Slot::read(&mut Reader::new(&encoded[10..]))?;
            if held.is_some_and(|stack| stack.id == 1 && stack.count > 0) {
                connection.send(&legacy17::bridge::outgoing(&encoded, outbound.entity)?)?;
                placed = true;
            }
        }
        if moved_at.is_some_and(|t: Instant| t.elapsed() >= Duration::from_secs(1))
            && evidence.keepalives >= keepalives_at_move + 2
        {
            println!(
                "{}",
                serde_json::json!({
                    "profile": profile.name, "protocol": version.protocol(),
                    "joined": evidence.joined, "position_acknowledged": evidence.positioned,
                    "fixture_teleport": evidence.fixture_position, "columns": evidence.columns,
                "states_decoded": evidence.states, "keepalives": evidence.keepalives,
                    "stone_anchor": evidence.anchor_seen, "red_wool_metadata": evidence.metadata_seen,
                    "placement_confirmed": evidence.placement, "break_confirmed": evidence.broken,
                    "movement_sent": [1.5, 64.0, 0.5], "rendering_tested": false,
                    "native_world_generation_tested": false, "online_authentication_tested": false,
                })
            );
            return Ok(());
        }
    }
    Err(format!("incomplete fixture: joined={} positioned={} columns={} teleport={} anchor={} metadata={} placed={} broken={} keepalives={} pose={pose:?}",
        evidence.joined, evidence.positioned, evidence.columns, evidence.fixture_position,
        evidence.anchor_seen, evidence.metadata_seen, evidence.placement, evidence.broken, evidence.keepalives).into())
}
