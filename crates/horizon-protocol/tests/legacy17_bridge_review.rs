//! Independent 1.7 wire fixtures for conversion into the renderer's numeric
//! protocol representation. Field layouts are checked against Cuberite's
//! Apache-2.0 1.7EOL implementation (72aa50f710200832f97a0022dc70c5cd531b6aea).
use horizon_protocol::{
    buf::Reader,
    legacy17::{
        Version,
        bridge::{Incoming, outgoing},
    },
    legacy18::metadata::{self, Value},
};

const UUID: u128 = 0x00112233445566778899aabbccddeeff;

fn player_spawn(with_properties: bool) -> Vec<u8> {
    let mut bytes = b"\xac\x02\x2400112233-4455-6677-8899-aabbccddeeff\x04Alex".to_vec();
    if with_properties {
        bytes.extend_from_slice(b"\x01\x08textures\x05value\x03sig");
    }
    // XYZ fixed-point: -1.5,64,2.5; yaw=64,pitch=-32; held item=267.
    bytes.extend_from_slice(&[
        255, 255, 255, 208, 0, 0, 8, 0, 0, 0, 0, 80, 64, 224, 1, 11, 0, 2, 0x66, 0x41, 0xa0, 0, 0,
        0x7f,
    ]);
    bytes
}

#[test]
fn player_spawn_version4_omits_properties_but_version5_carries_them() {
    for (version, properties) in [(Version::V1_7_2, false), (Version::V1_7_10, true)] {
        let mut bridge = Incoming::new(version);
        let input = player_spawn(properties);
        let result = bridge.translate(0x0c, &input).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, 0x38);
        let mut info = Reader::new(&result[0].1);
        assert_eq!((info.varint().unwrap(), info.varint().unwrap()), (0, 1));
        assert_eq!(info.uuid().unwrap(), UUID);
        assert_eq!(info.string(16).unwrap(), "Alex");
        assert_eq!(info.varint().unwrap(), i32::from(properties));
        if properties {
            assert_eq!(info.string(32).unwrap(), "textures");
            assert_eq!(info.string(32).unwrap(), "value");
            assert!(info.bool().unwrap());
            assert_eq!(info.string(32).unwrap(), "sig");
        }
        assert_eq!(
            (
                info.varint().unwrap(),
                info.varint().unwrap(),
                info.bool().unwrap()
            ),
            (0, 0, false)
        );
        assert_eq!(info.remaining(), 0);
        assert_eq!(result[1].0, 0x0c);
        let mut spawn = Reader::new(&result[1].1);
        assert_eq!(spawn.varint().unwrap(), 300);
        assert_eq!(spawn.uuid().unwrap(), UUID);
        assert_eq!(
            (
                spawn.i32().unwrap(),
                spawn.i32().unwrap(),
                spawn.i32().unwrap()
            ),
            (-48, 2048, 80)
        );
        assert_eq!(
            (
                spawn.i8().unwrap(),
                spawn.i8().unwrap(),
                spawn.i16().unwrap()
            ),
            (64, -32, 267)
        );
        assert_eq!(
            metadata::read(&mut spawn).unwrap(),
            vec![(0, Value::Byte(2)), (6, Value::Float(20.))]
        );
        assert_eq!(spawn.remaining(), 0);
        for end in 0..input.len() {
            assert!(
                Incoming::new(version)
                    .translate(0x0c, &input[..end])
                    .is_err()
            );
        }
    }
}

#[test]
fn a_legacy_latency_update_preserves_the_spawned_players_skin_properties() {
    let mut bridge = Incoming::new(Version::V1_7_10);
    bridge.translate(0x0c, &player_spawn(true)).unwrap();
    let update = bridge.translate(0x38, b"\x04Alex\x01\x01\x2c").unwrap();
    assert_eq!(update.len(), 1);
    assert_eq!(update[0].0, 0x38);
    let mut r = Reader::new(&update[0].1);
    // PlayerInfo action 2 updates latency only, preserving names and textures.
    assert_eq!((r.varint().unwrap(), r.varint().unwrap()), (2, 1));
    assert_eq!(r.uuid().unwrap(), UUID);
    assert_eq!(r.varint().unwrap(), 300);
    assert_eq!(r.remaining(), 0);
    let removal = bridge.translate(0x38, b"\x04Alex\x00\x00\x00").unwrap();
    let mut r = Reader::new(&removal[0].1);
    assert_eq!(
        (r.varint().unwrap(), r.varint().unwrap(), r.uuid().unwrap()),
        (4, 1, UUID)
    );
    assert_eq!(r.remaining(), 0);
    let readd = bridge.translate(0x38, b"\x04Alex\x01\x00\x0a").unwrap();
    assert_eq!(Reader::new(&readd[0].1).varint().unwrap(), 0);
}

#[test]
fn legacy_living_name_metadata_reaches_the_shared_renderer_indices() {
    let mut bridge = Incoming::new(Version::V1_7_10);
    let result = bridge
        .translate(
            0x1c,
            &[
                0, 0, 1, 44, // fixed i32 entity 300
                0x8a, 4, b'N', b'a', b'm', b'e', // String index 10
                0x0b, 1, // visible-name byte index 11
                0x66, 0x41, 0xa0, 0, 0, // float health index 6
                0x7f,
            ],
        )
        .unwrap();
    let mut r = Reader::new(&result[0].1);
    assert_eq!(r.varint().unwrap(), 300);
    assert_eq!(
        metadata::read(&mut r).unwrap(),
        vec![
            (2, Value::String("Name".into())),
            (3, Value::Byte(1)),
            (6, Value::Float(20.)),
        ]
    );
    assert_eq!(r.remaining(), 0);
    // Remapping must not produce duplicate destination fields.
    assert!(
        bridge
            .translate(0x1c, &[0, 0, 0, 1, 0x82, 1, b'a', 0x8a, 1, b'b', 0x7f])
            .is_err()
    );
}

#[test]
fn relative_entity_moves_use_fixed_ids_and_gain_only_the_missing_ground_flag() {
    let mut bridge = Incoming::new(Version::V1_7_10);
    assert_eq!(
        bridge.translate(0x15, &[0, 0, 1, 44, 255, 2, 128]).unwrap(),
        vec![(0x15, vec![172, 2, 255, 2, 128, 0])]
    );
    assert_eq!(
        bridge
            .translate(0x17, &[0, 0, 1, 44, 255, 2, 128, 64, 224])
            .unwrap(),
        vec![(0x17, vec![172, 2, 255, 2, 128, 64, 224, 0])]
    );
    assert_eq!(
        bridge
            .translate(0x13, &[2, 0, 0, 1, 44, 0, 0, 0, 1])
            .unwrap(),
        vec![(0x13, vec![2, 172, 2, 1])]
    );
}

#[test]
fn inventory_actions_retain_window_transaction_and_compressed_nbt_sentinel() {
    // Protocol47 click: window2, slot12, button1, transaction258, mode0, ID1 count3 damage7 and TAG_End.
    let click = [0x0e, 2, 0, 12, 1, 1, 2, 0, 0, 1, 3, 0, 7, 0];
    assert_eq!(
        outgoing(&click, 300).unwrap(),
        [0x0e, 2, 0, 12, 1, 1, 2, 0, 0, 1, 3, 0, 7, 255, 255]
    );
    assert_eq!(
        outgoing(&[0x0f, 2, 1, 2, 1], 300).unwrap(),
        [0x0f, 2, 1, 2, 1]
    );
    assert_eq!(outgoing(&[0x0a], 300).unwrap(), [0x0a, 0, 0, 1, 44, 1]);
    assert_eq!(
        outgoing(&[0x0b, 172, 2, 3, 0], 300).unwrap(),
        [0x0b, 0, 0, 1, 44, 4, 0, 0, 0, 0]
    );
    assert!(outgoing(&click[..click.len() - 1], 300).is_err());
}

#[test]
fn horse_windows_preserve_the_entity_id_after_the_slot_count() {
    // Window 4, historical type 11, custom title Steed, 17 slots, entity 300.
    // Both wire formats carry the horse entity as a final big-endian i32.
    let horse = b"\x04\x0b\x05Steed\x11\x01\x00\x00\x01\x2c";
    let expected = b"\x04\x0bEntityHorse\x10{\"text\":\"Steed\"}\x11\x00\x00\x01\x2c";
    for version in [Version::V1_7_2, Version::V1_7_10] {
        let mut bridge = Incoming::new(version);
        assert_eq!(
            bridge.translate(0x2d, horse).unwrap(),
            vec![(0x2d, expected.to_vec())]
        );
        for end in 0..horse.len() {
            assert!(bridge.translate(0x2d, &horse[..end]).is_err());
        }
        let mut trailing = horse.to_vec();
        trailing.push(0);
        assert!(bridge.translate(0x2d, &trailing).is_err());

        // Ordinary windows must not acquire an entity field.
        let chest = bridge.translate(0x2d, b"\x01\x00\x04Loot\x1b\x01").unwrap();
        assert_eq!(
            chest,
            vec![(
                0x2d,
                b"\x01\x0fminecraft:chest\x0f{\"text\":\"Loot\"}\x1b".to_vec()
            )]
        );
    }
}

#[test]
fn non_storage_workstations_use_the_required_zero_slot_count() {
    let mut bridge = Incoming::new(Version::V1_7_10);
    for (kind, slots, name) in [
        (1, 10, "minecraft:crafting_table"),
        (4, 2, "minecraft:enchanting_table"),
        (8, 3, "minecraft:anvil"),
    ] {
        let result = bridge.translate(0x2d, &[2, kind, 0, slots, 1]).unwrap();
        let mut r = Reader::new(&result[0].1);
        assert_eq!(r.u8().unwrap(), 2);
        assert_eq!(r.string(32).unwrap(), name);
        assert_eq!(r.string(32).unwrap(), "{\"text\":\"\"}");
        assert_eq!(r.u8().unwrap(), 0);
        assert_eq!(r.remaining(), 0);
    }
}
