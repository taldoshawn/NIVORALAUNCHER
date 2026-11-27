//! Protocol 4/5 field fixtures and actual loopback transport. These validate
//! network compatibility; they do not assert complete historical gameplay.
use horizon_protocol::{
    Error,
    buf::{Reader, Writer},
    connection::Connection,
    crypto::server_hash,
    legacy17::{
        self, LoginOutcome, LoginPacket, Version,
        actions::{self, Digging, EntityAction},
        play::{self, Packet, Pose},
    },
    rsa::RsaPrivateKey,
};
use std::{
    net::{TcpListener, TcpStream},
    time::Duration,
};

fn loopback() -> (Connection, Connection) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (server, _) = listener.accept().unwrap();
    let client = Connection::from_stream(client).unwrap();
    let server = Connection::from_stream(server).unwrap();
    server
        .reader
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    (client, server)
}

fn assert_hello(server: &mut Connection, protocol: u8) {
    let mut expected = vec![protocol];
    expected.extend_from_slice(b"\x09localhost\x63\xdd\x02");
    assert_eq!(server.recv().unwrap(), (0, expected));
    assert_eq!(server.recv().unwrap(), (0, b"\x04Alex".to_vec()));
}

fn success(compact: bool) -> Vec<u8> {
    let mut bytes = vec![2];
    if compact {
        bytes.extend_from_slice(b"\x2000112233445566778899aabbccddeeff");
    } else {
        bytes.extend_from_slice(b"\x2400112233-4455-6677-8899-aabbccddeeff");
    }
    bytes.extend_from_slice(b"\x04Alex");
    bytes
}

fn encryption_request(key: &RsaPrivateKey) -> Vec<u8> {
    let public = key.public_der();
    let mut request = vec![1, 0]; // Packet ID and empty server ID.
    request.extend_from_slice(&(public.len() as u16).to_be_bytes());
    request.extend_from_slice(&public);
    request.extend_from_slice(&[0, 4, 0xa1, 0xb2, 0xc3, 0xd4]);
    request
}

fn read_short_array(r: &mut Reader<'_>) -> Vec<u8> {
    let len = r.i16().unwrap();
    assert!(len > 0);
    r.bytes(len as usize).unwrap().to_vec()
}

#[test]
fn both_handshakes_enter_play_without_modern_ack_or_compression() {
    for (version, wire_protocol, compact) in
        [(Version::V1_7_2, 4, true), (Version::V1_7_10, 5, false)]
    {
        let (client, mut server) = loopback();
        let worker = std::thread::spawn(move || {
            assert_hello(&mut server, wire_protocol);
            server.send(&success(compact)).unwrap();
            server.send(&[0, 0xff, 0xff, 0xff, 0xff]).unwrap();
            // The first response must be an i32 keepalive, with no login ACK.
            assert_eq!(server.recv().unwrap(), (0, vec![0xff; 4]));
            server.send(b"\x40\x0e{\"text\":\"bye\"}").unwrap();
        });
        let (mut client, profile) = legacy17::login(
            version,
            client,
            "localhost",
            25565,
            "Alex",
            Duration::from_secs(5),
            |_| panic!("offline peer requested no authentication"),
        )
        .unwrap();
        assert_eq!(profile.uuid, 0x00112233445566778899aabbccddeeff);
        assert_eq!(profile.name, "Alex");
        let (id, payload) = client.recv().unwrap();
        assert_eq!(Packet::decode(id, &payload).unwrap(), Packet::KeepAlive(-1));
        client.send(&play::keep_alive(-1)).unwrap();
        let (id, payload) = client.recv().unwrap();
        assert_eq!(
            Packet::decode(id, &payload).unwrap(),
            Packet::Disconnect("{\"text\":\"bye\"}".into())
        );
        worker.join().unwrap();
    }
}

#[test]
fn encrypted_login_uses_short_arrays_and_authenticates_before_play() {
    let key = RsaPrivateKey::generate(1024).unwrap();
    let public = key.public_der();
    let (client, mut server) = loopback();
    let (send_hash, receive_hash) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        assert_hello(&mut server, 5);
        server.send(&encryption_request(&key)).unwrap();
        let (id, bytes) = server.recv().unwrap();
        assert_eq!(id, 1);
        // A 1024-bit RSA ciphertext has a 00 80 short prefix, not 80 01 VarInt.
        assert_eq!(&bytes[..2], &[0, 128]);
        let mut r = Reader::new(&bytes);
        let secret: [u8; 16] = key
            .decrypt(&read_short_array(&mut r))
            .unwrap()
            .try_into()
            .unwrap();
        assert_eq!(
            key.decrypt(&read_short_array(&mut r)).unwrap(),
            [0xa1, 0xb2, 0xc3, 0xd4]
        );
        assert_eq!(r.remaining(), 0);
        assert_eq!(
            receive_hash.recv_timeout(Duration::from_secs(5)).unwrap(),
            server_hash("", &secret, &public)
        );
        server.enable_encryption(&secret);
        server.send(&success(false)).unwrap();
        server.send(&[0, 0x12, 0x34, 0x56, 0x78]).unwrap();
        assert_eq!(server.recv().unwrap(), (0, vec![0x12, 0x34, 0x56, 0x78]));
    });
    let (mut client, _) = legacy17::login(
        Version::V1_7_10,
        client,
        "localhost",
        25565,
        "Alex",
        Duration::from_secs(5),
        |hash| {
            send_hash.send(hash.to_owned()).unwrap();
            Ok(())
        },
    )
    .unwrap();
    let (id, bytes) = client.recv().unwrap();
    assert_eq!(
        Packet::decode(id, &bytes).unwrap(),
        Packet::KeepAlive(0x12345678)
    );
    client.send(&play::keep_alive(0x12345678)).unwrap();
    worker.join().unwrap();
}

#[test]
fn authentication_failure_never_retries_offline() {
    let key = RsaPrivateKey::generate(1024).unwrap();
    let (client, mut server) = loopback();
    let worker = std::thread::spawn(move || {
        assert_hello(&mut server, 5);
        server.send(&encryption_request(&key)).unwrap();
        assert!(server.recv().unwrap_err().is_end_of_stream());
    });
    let result = legacy17::login(
        Version::V1_7_10,
        client,
        "localhost",
        25565,
        "Alex",
        Duration::from_secs(5),
        |_| Err(Error::new("session rejected")),
    );
    assert_eq!(result.err().unwrap().to_string(), "session rejected");
    worker.join().unwrap();
}

#[test]
fn login_preserves_disconnect_components_and_rejects_compression() {
    let reason = r#"{"text":"Acesso negado","extra":[{"text":"!"}]}"#;
    let (client, mut server) = loopback();
    let worker = std::thread::spawn(move || {
        assert_hello(&mut server, 5);
        let mut w = Writer::new();
        w.varint(0);
        w.string(reason);
        server.send(&w.data).unwrap();
        assert!(server.recv().unwrap_err().is_end_of_stream());
    });
    let outcome = legacy17::login_outcome(
        Version::V1_7_10,
        client,
        "localhost",
        25565,
        "Alex",
        Duration::from_secs(5),
        |_| panic!("unexpected authentication"),
    )
    .unwrap();
    match outcome {
        LoginOutcome::Disconnected(component) => assert_eq!(component, reason),
        LoginOutcome::Joined(..) => panic!("rejected login entered play"),
    }
    worker.join().unwrap();

    let (client, mut server) = loopback();
    let worker = std::thread::spawn(move || {
        assert_hello(&mut server, 5);
        server.send(&[3, 0]).unwrap();
        assert!(server.recv().unwrap_err().is_end_of_stream());
    });
    let result = legacy17::login(
        Version::V1_7_10,
        client,
        "localhost",
        25565,
        "Alex",
        Duration::from_secs(5),
        |_| Ok(()),
    );
    assert!(result.err().unwrap().to_string().contains("login packet 3"));
    worker.join().unwrap();
}

#[test]
fn repeated_encryption_is_rejected_without_authenticating_again() {
    let key = RsaPrivateKey::generate(1024).unwrap();
    let (client, mut server) = loopback();
    let worker = std::thread::spawn(move || {
        assert_hello(&mut server, 5);
        let request = encryption_request(&key);
        server.send(&request).unwrap();
        let (id, bytes) = server.recv().unwrap();
        assert_eq!(id, 1);
        let mut r = Reader::new(&bytes);
        let secret: [u8; 16] = key
            .decrypt(&read_short_array(&mut r))
            .unwrap()
            .try_into()
            .unwrap();
        server.enable_encryption(&secret);
        server.send(&request).unwrap();
        assert!(server.recv().unwrap_err().is_end_of_stream());
    });
    let mut callbacks = 0;
    let result = legacy17::login(
        Version::V1_7_10,
        client,
        "localhost",
        25565,
        "Alex",
        Duration::from_secs(5),
        |_| {
            callbacks += 1;
            Ok(())
        },
    );
    assert!(result.err().unwrap().to_string().contains("repeated"));
    assert_eq!(callbacks, 1);
    worker.join().unwrap();
}

#[test]
fn login_boundaries_and_negotiation_deadline_are_enforced() {
    for compact in [false, true] {
        let bytes = success(compact);
        for end in 1..bytes.len() {
            assert!(LoginPacket::decode(2, &bytes[1..end]).is_err());
        }
        let mut trailing = bytes[1..].to_vec();
        trailing.push(0);
        assert!(LoginPacket::decode(2, &trailing).is_err());
    }
    for bytes in [
        &[0, 0xff, 0xff][..],
        &[0, 0x20, 0x01],
        &[0, 0, 0],
        &[0, 0, 1, 1, 0, 0],
    ] {
        assert!(LoginPacket::decode(1, bytes).is_err());
    }
    let mut bad_uuid = success(false)[1..].to_vec();
    bad_uuid[9] = b'0';
    assert!(LoginPacket::decode(2, &bad_uuid).is_err());
    let (client, _server) = loopback();
    assert!(
        legacy17::login(
            Version::V1_7_10,
            client,
            "localhost",
            25565,
            "Alex",
            Duration::from_millis(20),
            |_| Ok(())
        )
        .err()
        .unwrap()
        .is_timed_out()
    );
    for (host, name) in [
        ("", "Alex"),
        ("bad\0host", "Alex"),
        ("localhost", "bad name"),
        ("localhost", ""),
    ] {
        let (client, mut server) = loopback();
        assert!(
            legacy17::login(
                Version::V1_7_10,
                client,
                host,
                25565,
                name,
                Duration::from_secs(1),
                |_| Ok(())
            )
            .is_err()
        );
        assert!(server.recv().unwrap_err().is_end_of_stream());
    }
}

#[test]
fn play_packets_use_17_widths_and_reject_truncation_and_trailing_bytes() {
    let cases = [
        (0x00, vec![0x80, 0, 0, 0], Packet::KeepAlive(i32::MIN)),
        (
            0x01,
            b"\0\0\0\x2a\x01\xff\x02\x14\x07default".to_vec(),
            Packet::Join {
                entity: 42,
                gamemode: 1,
                dimension: -1,
                difficulty: 2,
                max_players: 20,
                level_type: "default".into(),
            },
        ),
        (0x02, b"\x02{}".to_vec(), Packet::Chat { json: "{}".into() }),
        (
            0x06,
            vec![0x41, 0xa0, 0, 0, 0, 20, 0x40, 0xa0, 0, 0],
            Packet::Health {
                health: 20.,
                food: 20,
                saturation: 5.,
            },
        ),
        (
            0x12,
            vec![0, 0, 1, 0x2c, 0x1f, 0x40, 0xe0, 0xc0, 0, 0],
            Packet::Velocity {
                entity: 300,
                x: 8000,
                y: -8000,
                z: 0,
            },
        ),
        (
            0x23,
            vec![0xff, 0xff, 0xff, 0xff, 255, 0, 0, 0, 2, 0x80, 0x02, 15],
            Packet::BlockChange {
                x: -1,
                y: 255,
                z: 2,
                state: 0x100f,
            },
        ),
    ];
    for (id, bytes, expected) in cases {
        assert_eq!(Packet::decode(id, &bytes).unwrap(), expected);
        for end in 0..bytes.len() {
            assert!(Packet::decode(id, &bytes[..end]).is_err());
        }
        let mut trailing = bytes;
        trailing.push(0);
        assert!(Packet::decode(id, &trailing).is_err());
    }
    assert!(Packet::decode(0x09, &[9]).is_err());
    assert!(Packet::decode(0x06, &[0x7f, 0x80, 0, 0, 0, 20, 0, 0, 0, 0]).is_err());
    assert!(Packet::decode(0x03, &[0, 0, 0, 0, 0, 0, 0, 0, 0x80, 0, 0, 0, 0, 0, 0, 0]).is_err());
    assert!(Packet::decode(0x23, &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 16]).is_err());
}

#[test]
fn teleports_normalize_eye_y_and_movement_restores_both_vertical_coordinates() {
    // X=1.25, eye Y=64 + promoted Java float(1.62), Z=-2.5, yaw=90, pitch=-30.
    let bytes = [
        0x3f, 0xf4, 0, 0, 0, 0, 0, 0, 0x40, 0x50, 0x67, 0xae, 0x14, 0x80, 0, 0, 0xc0, 0x04, 0, 0,
        0, 0, 0, 0, 0x42, 0xb4, 0, 0, 0xc1, 0xf0, 0, 0, 1,
    ];
    let packet = Packet::decode(8, &bytes).unwrap();
    let expected = Pose {
        x: 1.25,
        y: 64.,
        z: -2.5,
        yaw: 90.,
        pitch: -30.,
    };
    let old = Pose {
        x: 999.,
        y: 999.,
        z: 999.,
        yaw: 1.,
        pitch: 2.,
    };
    assert_eq!(old.corrected(&packet).unwrap(), expected);
    let outgoing = expected.encode(true).unwrap();
    assert_eq!(&outgoing[..9], &[6, 0x3f, 0xf4, 0, 0, 0, 0, 0, 0]);
    assert_eq!(&outgoing[9..17], &[0x40, 0x50, 0, 0, 0, 0, 0, 0]);
    assert_eq!(&outgoing[17..25], &bytes[8..16]);
    assert_eq!(&outgoing[25..], &bytes[16..]);
    assert_eq!(outgoing.len(), 42);
    let mut invalid = bytes;
    invalid[..8].copy_from_slice(&f64::INFINITY.to_be_bytes());
    assert!(Packet::decode(8, &invalid).is_err());
    assert!(
        Pose {
            x: f64::NAN,
            ..expected
        }
        .encode(false)
        .is_err()
    );
}

#[test]
fn action_fixtures_preserve_fixed_entity_ids_and_separate_block_coordinates() {
    assert_eq!(
        actions::digging(Digging::Start, -1, 64, 2, 1).unwrap(),
        [7, 0, 255, 255, 255, 255, 64, 0, 0, 0, 2, 1]
    );
    assert_eq!(
        actions::interact_entity(300, true).unwrap(),
        [2, 0, 0, 1, 44, 1]
    );
    assert_eq!(
        actions::entity_action(300, EntityAction::StartSprinting, 0).unwrap(),
        [11, 0, 0, 1, 44, 4, 0, 0, 0, 0]
    );
    assert_eq!(actions::swing_arm(300).unwrap(), [10, 0, 0, 1, 44, 1]);
    assert_eq!(actions::held_slot(8).unwrap(), [9, 0, 8]);
    assert_eq!(
        actions::place(1, 64, -2, 1, None, [8, 16, 0]).unwrap(),
        [8, 0, 0, 0, 1, 64, 255, 255, 255, 254, 1, 255, 255, 8, 16, 0]
    );
    assert_eq!(
        actions::use_item(None).unwrap(),
        [
            8, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0
        ]
    );
    assert_eq!(
        actions::client_settings("en_US", 4, 0, true, 2, true).unwrap(),
        b"\x15\x05en_US\x04\x00\x01\x02\x01"
    );
    assert_eq!(
        actions::plugin_message("MC|Brand", b"Nivora").unwrap(),
        b"\x17\x08MC|Brand\x00\x06Nivora"
    );
    assert!(actions::digging(Digging::Start, 0, 256, 0, 0).is_err());
    assert!(actions::digging(Digging::Start, 0, 1, 0, 6).is_err());
    assert!(actions::entity_action(1, EntityAction::StartSneaking, 1).is_err());
    assert!(actions::place(0, 1, 0, 0, None, [17, 0, 0]).is_err());
    assert!(actions::swing_arm(-1).is_err());
    assert!(actions::position(0., f64::NAN, 0., false).is_err());
    assert!(actions::look(f32::INFINITY, 0., false).is_err());
    assert!(actions::client_settings("en_US", 4, 3, true, 2, true).is_err());
    assert!(actions::creative_slot(45, None).is_err());
    assert!(actions::plugin_message("MC|Brand", &vec![0; 32768]).is_err());
}

#[test]
fn outgoing_chat_counts_java_utf16_units() {
    assert!(play::chat(&"😀".repeat(50)).is_ok());
    assert!(play::chat(&"😀".repeat(51)).is_err());
    for invalid in ["line\nbreak", "§aformatted", "bad\u{7f}"] {
        assert!(play::chat(invalid).is_err());
    }
    assert_eq!(play::chat("olá").unwrap(), b"\x01\x04ol\xc3\xa1");
}
