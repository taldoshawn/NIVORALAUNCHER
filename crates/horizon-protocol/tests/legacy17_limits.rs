//! Adversarial wire fixtures exercise aggregate adapter quotas, not just each
//! individual chunk/slot decoder's decompression limit.
use flate2::{
    Compression,
    write::{GzEncoder, ZlibEncoder},
};
use horizon_protocol::{
    buf::Reader,
    legacy17::{Version, bridge::Incoming},
    legacy18,
};
use std::io::Write;

fn empty_bulk(count: i16) -> Vec<u8> {
    // Each empty column still carries 256 biome bytes; no section arrays.
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&vec![1; count as usize * 256]).unwrap();
    let compressed = encoder.finish().unwrap();
    let mut packet = count.to_be_bytes().to_vec();
    packet.extend_from_slice(&(compressed.len() as i32).to_be_bytes());
    packet.push(1); // Overworld skylight.
    packet.extend_from_slice(&compressed);
    for x in 0..i32::from(count) {
        packet.extend_from_slice(&x.to_be_bytes());
        packet.extend_from_slice(&0i32.to_be_bytes());
        packet.extend_from_slice(&[0; 4]); // Primary and Add masks.
    }
    packet
}

#[test]
fn bulk_air_conversion_obeys_aggregate_output_budget() {
    let mut incoming = Incoming::new(Version::V1_7_10);
    let small = incoming.translate(0x26, &empty_bulk(2)).unwrap();
    assert_eq!(small.len(), 2);
    for (id, bytes) in &small {
        assert_eq!(*id, 0x21);
        let column = legacy18::chunk::Column::decode(bytes, true).unwrap();
        assert!(!column.is_unload());
        assert_eq!(column.section(0).unwrap().state(0, 0, 0), Some(0));
    }
    // 700 columns decompress to only 179200 bytes, but their renderer
    // representation needs an explicit air section each and exceeds 8 MiB.
    let bomb = empty_bulk(700);
    assert!(bomb.len() < 16 * 1024);
    let error = incoming.translate(0x26, &bomb).unwrap_err();
    assert!(error.to_string().contains("expansion"));
    // A failed bulk does not poison later valid packets.
    assert_eq!(incoming.translate(0x26, &empty_bulk(1)).unwrap().len(), 1);
}

fn inventory_with_compressed_items(count: i16) -> Vec<u8> {
    // A hand-written named compound containing a 1 MiB byte array named "d".
    // Every item is individually below the 2 MiB NBT quota.
    let mut nbt = vec![10, 0, 0, 7, 0, 1, b'd', 0, 0x10, 0, 0];
    nbt.resize(nbt.len() + 1024 * 1024, 0);
    nbt.push(0);
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&nbt).unwrap();
    let compressed = encoder.finish().unwrap();
    assert!(compressed.len() < i16::MAX as usize);
    let mut packet = vec![0]; // Player inventory window.
    packet.extend_from_slice(&count.to_be_bytes());
    for _ in 0..count {
        packet.extend_from_slice(&[0, 1, 1, 0, 0]); // Stone, one item, damage zero.
        packet.extend_from_slice(&(compressed.len() as i16).to_be_bytes());
        packet.extend_from_slice(&compressed);
    }
    packet
}

#[test]
fn individually_valid_gzip_slots_cannot_expand_an_inventory_without_bound() {
    let mut incoming = Incoming::new(Version::V1_7_10);
    let valid = incoming
        .translate(0x30, &inventory_with_compressed_items(1))
        .unwrap();
    assert_eq!(valid.len(), 1);
    let mut reader = Reader::new(&valid[0].1);
    assert_eq!(reader.u8().unwrap(), 0);
    assert_eq!(reader.i16().unwrap(), 1);
    let item = legacy18::slot::Slot::read(&mut reader).unwrap().unwrap();
    assert_eq!(item.id, 1);
    assert_eq!(reader.remaining(), 0);
    let bomb = inventory_with_compressed_items(9);
    assert!(bomb.len() < 16 * 1024);
    let error = incoming.translate(0x30, &bomb).unwrap_err();
    assert!(error.to_string().contains("inventory expansion"));
    assert!(incoming.translate(0x30, &[0, 0, 1, 255, 255]).is_ok());
}

fn player_list(name: &str, online: bool) -> Vec<u8> {
    assert!(name.len() < 128);
    let mut packet = vec![name.len() as u8];
    packet.extend_from_slice(name.as_bytes());
    packet.push(u8::from(online));
    packet.extend_from_slice(&20i16.to_be_bytes());
    packet
}

fn player_spawn(name: &str) -> Vec<u8> {
    let uuid = b"00112233445566778899aabbccddeeff";
    let mut packet = vec![1, uuid.len() as u8]; // Entity ID, UUID string length.
    packet.extend_from_slice(uuid);
    packet.push(name.len() as u8);
    packet.extend_from_slice(name.as_bytes());
    packet.push(0); // Protocol 5 property count.
    packet.extend_from_slice(&[0; 16]); // XYZ, yaw, pitch, held item.
    packet.push(0x7f); // End of entity metadata.
    packet
}

#[test]
fn player_name_quota_allows_updates_removals_and_replacement_profiles() {
    let mut incoming = Incoming::new(Version::V1_7_10);
    for index in 0..4096 {
        incoming
            .translate(0x38, &player_list(&format!("u{index:04}"), true))
            .unwrap();
    }
    // Updating an existing player is not a new allocation.
    let update = incoming
        .translate(0x38, &player_list("u0000", true))
        .unwrap();
    assert_eq!(Reader::new(&update[0].1).varint().unwrap(), 2);
    let error = incoming
        .translate(0x38, &player_list("u4096", true))
        .unwrap_err();
    assert!(error.to_string().contains("session budget"));
    assert!(incoming.translate(0x0c, &player_spawn("u4096")).is_err());
    // Server-supplied UUID replaces the placeholder without increasing names.
    assert!(incoming.translate(0x0c, &player_spawn("u0001")).is_ok());
    incoming
        .translate(0x38, &player_list("u0000", false))
        .unwrap();
    assert!(incoming.translate(0x0c, &player_spawn("u4096")).is_ok());
    assert!(
        incoming
            .translate(0x38, &player_list("u4097", true))
            .is_err()
    );
}

#[test]
fn truncated_and_extra_bytes_do_not_return_partially_normalized_packets() {
    let mut incoming = Incoming::new(Version::V1_7_10);
    let valid = empty_bulk(1);
    for length in [0, 1, 2, 6, valid.len() - 1] {
        assert!(incoming.translate(0x26, &valid[..length]).is_err());
    }
    let mut trailing = valid;
    trailing.push(0);
    assert!(incoming.translate(0x26, &trailing).is_err());
    // Two items announced but only the first empty item is present.
    assert!(incoming.translate(0x30, &[0, 0, 2, 255, 255]).is_err());
    assert!(incoming.translate(0x30, &[0, 255, 255]).is_err());
    assert!(incoming.translate(0x30, &[0, 0, 1, 255, 255, 0]).is_err());
    let named = player_list("valid", true);
    for length in 0..named.len() {
        assert!(incoming.translate(0x38, &named[..length]).is_err());
    }
    assert!(incoming.translate(0x38, &named).is_ok());
}

#[test]
fn client_settings_reach_protocol_5_with_historical_difficulty_and_cape_fields() {
    use horizon_protocol::packets::{configuration::ClientInformation, play::Serverbound};
    for (skin_parts, cape) in [(0x40, 0), (0x41, 1)] {
        let mut settings = ClientInformation::new(6);
        settings.language = "pt_BR".into();
        settings.chat_visibility = 2;
        settings.chat_colors = false;
        settings.model_customisation = skin_parts;
        let mut model = legacy18::bridge::Outbound::default();
        let internal = model
            .encode(&Serverbound::ClientInformation(settings))
            .unwrap()
            .unwrap();
        let actual = horizon_protocol::legacy17::bridge::outgoing(&internal, 7).unwrap();
        // ID, locale string, distance, visibility, colors, difficulty, show cape.
        // Newer skin-part bits and modern client fields are not sent on this wire.
        assert_eq!(
            actual,
            [0x15, 5, b'p', b't', b'_', b'B', b'R', 6, 2, 0, 2, cape]
        );
    }
}
