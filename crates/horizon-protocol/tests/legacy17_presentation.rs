use horizon_protocol::legacy17::{
    actions,
    presentation::{self, MapIcon, MapUpdate, Packet},
};

fn collect() -> Vec<u8> {
    vec![0, 0, 1, 2, 0, 1, 0, 3]
}

fn sound() -> Vec<u8> {
    // random.orb at (1.5, 64, -0.5), volume .25, unsigned pitch 128.
    let mut bytes = b"\x0arandom.orb".to_vec();
    bytes.extend([0, 0, 0, 12, 0, 0, 2, 0, 255, 255, 255, 252]);
    bytes.extend([0x3e, 0x80, 0, 0, 128]);
    bytes
}

fn sign() -> Vec<u8> {
    vec![
        255, 255, 255, 252, 0, 70, 0, 0, 0, 9, 2, b'H', b'i', 0, 1, b'x', 0,
    ]
}

fn brand() -> Vec<u8> {
    b"\x08MC|Brand\x00\x07vanilla".to_vec()
}

#[test]
fn entity_presentation_uses_fixed_ids_and_minus_one_detach() {
    assert_eq!(
        Packet::decode(0x0d, &collect()).unwrap(),
        Some(Packet::Collect {
            item: 258,
            collector: 65539,
        })
    );
    assert_eq!(
        Packet::decode(0x1b, &[0, 0, 1, 2, 255, 255, 255, 255, 0]).unwrap(),
        Some(Packet::Attach {
            entity: 258,
            vehicle: -1,
            leash: false
        })
    );
    assert_eq!(
        Packet::decode(0x1b, &[0, 0, 1, 2, 0, 0, 0, 0, 1]).unwrap(),
        Some(Packet::Attach {
            entity: 258,
            vehicle: 0,
            leash: true
        })
    );
    assert!(Packet::decode(0x1b, &[0, 0, 0, 1, 0, 0, 0, 2, 2]).is_err());
    assert!(Packet::decode(0x1b, &[0, 0, 0, 1, 255, 255, 255, 254, 0]).is_err());
    assert!(Packet::decode(0x0d, &[255; 8]).is_err());
}

#[test]
fn named_sound_retains_fixed_coordinates_and_unsigned_pitch() {
    assert_eq!(
        Packet::decode(0x29, &sound()).unwrap(),
        Some(Packet::Sound {
            name: "random.orb".into(),
            position: [12, 512, -4],
            volume: 0.25,
            pitch: 128.0 / 63.0,
        })
    );
    for invalid in [f32::NAN, f32::INFINITY, -0.01] {
        let mut bytes = sound();
        let offset = bytes.len() - 5;
        bytes[offset..offset + 4].copy_from_slice(&invalid.to_be_bytes());
        assert!(Packet::decode(0x29, &bytes).is_err());
    }
    assert_eq!(
        presentation::sound_name("random.orb"),
        Some("minecraft:entity.experience_orb.pickup")
    );
    assert_eq!(
        presentation::sound_name("minecraft:random.chestclosed"),
        Some("minecraft:block.chest.close")
    );
    assert_eq!(presentation::sound_name("custom:random.orb"), None);
    assert_eq!(presentation::sound_name("mob.unknown.hurt"), None);
}

#[test]
fn historical_map_updates_have_short_lengths_and_type_high_nibble() {
    // ID 128, payload length 5, column x9 beginning y126 with two colors.
    assert_eq!(
        Packet::decode(0x34, &[128, 1, 0, 5, 0, 9, 126, 8, 9]).unwrap(),
        Some(Packet::Map {
            id: 128,
            update: MapUpdate::Column {
                x: 9,
                y: 126,
                colors: vec![8, 9]
            }
        })
    );
    assert_eq!(
        Packet::decode(0x34, &[7, 0, 7, 1, 0x34, 128, 127, 6, 1, 255]).unwrap(),
        Some(Packet::Map {
            id: 7,
            update: MapUpdate::Icons(vec![
                MapIcon {
                    kind: 3,
                    rotation: 4,
                    x: -128,
                    y: 127
                },
                MapIcon {
                    kind: 0,
                    rotation: 6,
                    x: 1,
                    y: -1
                },
            ])
        })
    );
    assert_eq!(
        Packet::decode(0x34, &[7, 0, 1, 1]).unwrap(),
        Some(Packet::Map {
            id: 7,
            update: MapUpdate::Icons(vec![])
        })
    );
    assert_eq!(
        Packet::decode(0x34, &[172, 2, 0, 2, 2, 4]).unwrap(),
        Some(Packet::Map {
            id: 300,
            update: MapUpdate::Scale(4)
        })
    );
    for bytes in [
        vec![7, 255, 255],                        // Negative signed length.
        vec![7, 0, 0],                            // Empty map submessage.
        vec![7, 0, 3, 0, 0, 0],                   // Empty column.
        vec![7, 0, 5, 0, 0, 127, 1, 2],           // Crosses the last row.
        vec![7, 0, 4, 0, 128, 0, 1],              // Invalid x.
        vec![7, 0, 2, 1, 1],                      // Partial icon triple.
        vec![7, 0, 2, 2, 5],                      // Scale above four.
        vec![7, 0, 3, 2, 1, 0],                   // Extra bytes inside a scale update.
        vec![7, 0, 1, 3],                         // Unimplemented submessage type.
        vec![255, 255, 255, 255, 15, 0, 2, 2, 1], // Negative map ID.
    ] {
        assert!(Packet::decode(0x34, &bytes).is_err(), "accepted {bytes:?}");
    }
}

#[test]
fn sign_text_is_plain_with_short_y_and_native_reply_limits() {
    let lines = ["Hi".into(), String::new(), "x".into(), String::new()];
    assert_eq!(
        Packet::decode(0x33, &sign()).unwrap(),
        Some(Packet::Sign {
            position: [-4, 70, 9],
            lines: lines.clone(),
        })
    );
    assert_eq!(
        actions::sign_update([-4, 70, 9], &lines).unwrap(),
        [vec![0x12], sign()].concat()
    );
    assert_eq!(
        Packet::decode(0x36, &[255, 255, 255, 252, 0, 0, 0, 70, 0, 0, 0, 9]).unwrap(),
        Some(Packet::OpenSign {
            position: [-4, 70, 9]
        })
    );
    let mut utf16 = [
        "😀".repeat(7) + "a",
        String::new(),
        String::new(),
        String::new(),
    ];
    let accepted = actions::sign_update([1, 255, 2], &utf16).unwrap();
    assert!(Packet::decode(0x33, &accepted[1..]).is_ok());
    utf16[0] = "😀".repeat(8);
    assert!(actions::sign_update([1, 70, 2], &utf16).is_err());
    let mut too_long = vec![0; 10];
    too_long.push(16);
    too_long.extend([b'x'; 16]);
    too_long.extend([0; 3]);
    assert!(Packet::decode(0x33, &too_long).is_err());
    assert!(actions::sign_update([1, 256, 2], &lines).is_err());
    assert!(Packet::decode(0x36, &[0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 0, 2]).is_err());
}

#[test]
fn plugin_messages_use_signed_lengths_and_raw_utf8_brand() {
    assert_eq!(
        Packet::decode(0x3f, &brand()).unwrap(),
        Some(Packet::Plugin {
            channel: "MC|Brand".into(),
            data: b"vanilla".to_vec(),
        })
    );
    assert_eq!(
        presentation::server_brand("MC|Brand", b"vanilla").unwrap(),
        Some("vanilla".into())
    );
    assert!(presentation::server_brand("MC|Brand", &[255]).is_err());
    assert_eq!(presentation::server_brand("CUSTOM", &[255]).unwrap(), None);
    assert_eq!(
        actions::plugin_message("MC|Brand", b"Nivora Rust").unwrap(),
        b"\x17\x08MC|Brand\x00\x0bNivora Rust"
    );
    assert!(Packet::decode(0x3f, b"\x01x\xff\xff").is_err());
    assert!(Packet::decode(0x3f, b"\x00\x00\x00").is_err());
    assert!(actions::plugin_message("x", &vec![0; 32768]).is_err());
}

#[test]
fn owned_frames_reject_every_truncation_and_extra_byte() {
    for (id, bytes) in [
        (0x0d, collect()),
        (0x1b, vec![0, 0, 0, 1, 0, 0, 0, 2, 0]),
        (0x29, sound()),
        (0x33, sign()),
        (0x34, vec![7, 0, 2, 2, 1]),
        (0x36, vec![0, 0, 0, 1, 0, 0, 0, 64, 0, 0, 0, 2]),
        (0x3f, brand()),
    ] {
        assert!(Packet::decode(id, &bytes).unwrap().is_some());
        for end in 0..bytes.len() {
            assert!(
                Packet::decode(id, &bytes[..end]).is_err(),
                "packet {id:#x} accepted prefix {end}"
            );
        }
        let mut extra = bytes;
        extra.push(0);
        assert!(
            Packet::decode(id, &extra).is_err(),
            "packet {id:#x} accepted trailing byte"
        );
    }
    assert_eq!(Packet::decode(0x2a, &[0, 255]).unwrap(), None);
}
