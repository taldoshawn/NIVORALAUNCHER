//! Hand-built 1.7 wire layouts; expected numeric states do not use the decoder.
use flate2::{
    Compression,
    write::{GzEncoder, ZlibEncoder},
};
use horizon_protocol::{
    buf::{NBT_QUOTA, Reader, Writer},
    legacy17::{
        chunk::{Column, decode_bulk},
        slot::{self, Slot},
    },
    legacy18,
};
use horizon_world::nbt::{self, Tag};
use std::io::Write;

fn zlib(data: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

fn column_packet(ground_up: bool, mask: u16, add: u16, compressed: &[u8]) -> Vec<u8> {
    let mut packet = Vec::new();
    packet.extend_from_slice(&(-7i32).to_be_bytes());
    packet.extend_from_slice(&12i32.to_be_bytes());
    packet.push(u8::from(ground_up));
    packet.extend_from_slice(&mask.to_be_bytes());
    packet.extend_from_slice(&add.to_be_bytes());
    packet.extend_from_slice(&(compressed.len() as i32).to_be_bytes());
    packet.extend_from_slice(compressed);
    packet
}

fn bulk_packet(sky: bool, headers: &[(i32, i32, u16, u16)], compressed: &[u8]) -> Vec<u8> {
    let mut packet = Vec::new();
    packet.extend_from_slice(&(headers.len() as i16).to_be_bytes());
    packet.extend_from_slice(&(compressed.len() as i32).to_be_bytes());
    packet.push(u8::from(sky));
    packet.extend_from_slice(compressed);
    for &(x, z, mask, add) in headers {
        packet.extend_from_slice(&x.to_be_bytes());
        packet.extend_from_slice(&z.to_be_bytes());
        packet.extend_from_slice(&mask.to_be_bytes());
        packet.extend_from_slice(&add.to_be_bytes());
    }
    packet
}

#[test]
fn sparse_sections_preserve_high_ids_metadata_and_each_light_nibble() {
    // Two sections at Y=0 and Y=15; only Y=15 has 12-bit block IDs.
    // The arrays are grouped by field, rather than interleaved per section.
    let mut raw = Vec::new();
    for (value, size) in [
        (0x23, 4096),
        (0x45, 4096), // Low block IDs
        (0x21, 2048),
        (0xba, 2048), // Metadata
        (0x65, 2048),
        (0x87, 2048), // Block light
        (0xed, 2048),
        (0xcb, 2048), // Sky light
        (0x0f, 2048), // High block-ID nibbles of section 15
        (42, 256),    // Biomes
    ] {
        raw.extend(std::iter::repeat_n(value, size));
    }
    let column = Column::decode(&column_packet(true, 0x8001, 0x8000, &zlib(&raw)), true).unwrap();
    assert_eq!((column.x, column.z), (-7, 12));
    assert_eq!(column.biomes().unwrap(), &[42; 256]);
    assert!(!column.is_unload());
    assert!(column.section(1).is_none());
    assert!(column.section(16).is_none());
    let low = column.section(0).unwrap();
    let high = column.section(15).unwrap();
    assert_eq!(low.state(0, 0, 0), Some(0x0231));
    assert_eq!(low.state(1, 0, 0), Some(0x0232));
    assert_eq!(high.state(0, 0, 0), Some(0xf45a));
    assert_eq!(high.state(15, 15, 15), Some(0x045b));
    assert_eq!(high.state(16, 0, 0), None);
    assert_eq!(low.block_light(0, 0, 0), Some(5));
    assert_eq!(low.sky_light(0, 0, 0), Some(13));
    assert_eq!(
        high.packed_light().take(2).collect::<Vec<_>>(),
        [0xb7, 0xc8]
    );
    assert_eq!(high.states().count(), 4096);
    assert_eq!(low.uniform_light(), None);
    let adapted = column.to_protocol47_payload();
    let common = legacy18::chunk::Column::decode(&adapted, true).unwrap();
    assert_eq!(common.section(15).unwrap().state(0, 0, 0), Some(0xf45a));
    assert_eq!(common.section(0).unwrap().sky_light(1, 0, 0), Some(14));
    assert_eq!(common.biomes(), Some([42u8; 256].as_slice()));
}

#[test]
fn partial_nether_column_uses_dimension_without_biomes_or_skylight() {
    let mut raw = vec![1; 4096];
    raw.extend_from_slice(&[0; 2048]);
    raw.extend_from_slice(&[0x77; 2048]);
    let packet = column_packet(false, 8, 0, &zlib(&raw));
    let column = Column::decode(&packet, false).unwrap();
    let section = column.section(3).unwrap();
    assert_eq!(section.state(15, 15, 15), Some(16));
    assert_eq!(section.sky_light(0, 0, 0), None);
    assert_eq!(section.uniform_light(), Some(7));
    assert_eq!(column.biomes(), None);
    assert!(Column::decode(&packet, true).is_err());
    let adapted = column.to_protocol47_payload();
    let common = legacy18::chunk::Column::decode(&adapted, false).unwrap();
    assert_eq!(common.section(3).unwrap().state(15, 15, 15), Some(16));
}

#[test]
fn standalone_unloads_accept_empty_and_compressed_forms() {
    for compressed in [vec![], zlib(&[]), zlib(&[1; 256])] {
        let column = Column::decode(&column_packet(true, 0, 0, &compressed), true).unwrap();
        assert!(column.is_unload());
        assert!(column.biomes().is_none());
        let adapted = column.to_protocol47_payload();
        assert!(
            legacy18::chunk::Column::decode(&adapted, true)
                .unwrap()
                .is_unload()
        );
    }
    let no_change = Column::decode(&column_packet(false, 0, 0, &zlib(&[])), true).unwrap();
    assert!(!no_change.is_unload());
    assert!(Column::decode(&column_packet(true, 0, 1, &[]), true).is_err());
}

#[test]
fn bulk_headers_follow_compressed_data_and_empty_columns_remain_loaded() {
    let mut raw = vec![0; 10240 + 256 + 256];
    raw[..4096].fill(35);
    raw[4096..6144].fill(0xee);
    raw[8192..10240].fill(0xff);
    raw[10240..10496].fill(2);
    raw[10496..].fill(9);
    let headers = [(-2, 7, 1, 0), (5, -4, 0, 0)];
    let packet = bulk_packet(true, &headers, &zlib(&raw));
    let columns = decode_bulk(&packet).unwrap();
    assert_eq!(columns.len(), 2);
    assert_eq!((columns[0].x, columns[0].z), (-2, 7));
    assert_eq!(columns[0].section(0).unwrap().state(0, 0, 0), Some(0x23e));
    assert_eq!(columns[0].biomes(), Some([2u8; 256].as_slice()));
    assert_eq!((columns[1].x, columns[1].z), (5, -4));
    assert!(!columns[1].is_unload());
    assert_eq!(columns[1].biomes(), Some([9u8; 256].as_slice()));
    let adapted = columns[1].to_protocol47_payload();
    let common = legacy18::chunk::Column::decode(&adapted, true).unwrap();
    assert!(!common.is_unload());
    assert_eq!(common.section(0).unwrap().state(0, 0, 0), Some(0));
    assert_eq!(common.section(0).unwrap().uniform_light(), Some(0xf0));
    assert_eq!(common.biomes(), Some([9u8; 256].as_slice()));
    let mut truncated = packet.clone();
    truncated.pop();
    assert!(decode_bulk(&truncated).is_err());
    let mut trailing = packet;
    trailing.push(0);
    assert!(decode_bulk(&trailing).is_err());
    assert!(decode_bulk(&bulk_packet(false, &[(0, 0, 0, 1)], &zlib(&[0; 256]))).is_err());
}

#[test]
fn malformed_or_oversized_chunk_compression_is_rejected() {
    let raw = vec![0; 10240 + 256];
    let valid = zlib(&raw);
    let mut bad_checksum = valid.clone();
    *bad_checksum.last_mut().unwrap() ^= 1;
    let mut trailing = valid.clone();
    trailing.push(0);
    let mut second_stream = valid.clone();
    second_stream.extend(zlib(&[]));
    for compressed in [
        valid[..valid.len() - 1].to_vec(),
        bad_checksum,
        trailing,
        second_stream,
        zlib(&raw[..raw.len() - 1]),
        zlib(&vec![0; raw.len() + 1]),
        zlib(&vec![0; 3 * 1024 * 1024]),
    ] {
        assert!(Column::decode(&column_packet(true, 1, 0, &compressed), true).is_err());
    }
    // Reject the declared inflation before allocating/reading any column body.
    let headers = vec![(0, 0, 0xffff, 0xffff); 43];
    let error = decode_bulk(&bulk_packet(true, &headers, &[])).unwrap_err();
    assert!(error.to_string().contains("inflation budget"));
    let mut negative = column_packet(true, 1, 0, &[]);
    negative[13..17].copy_from_slice(&(-1i32).to_be_bytes());
    assert!(Column::decode(&negative, true).is_err());
    assert!(Column::decode(&negative[..16], true).is_err());
}

fn tagged_slot(compressed: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0, 35, 12, 0, 14];
    bytes.extend_from_slice(&(compressed.len() as i16).to_be_bytes());
    bytes.extend_from_slice(compressed);
    bytes
}

#[test]
fn empty_and_untagged_slots_keep_packet_boundaries() {
    let bytes = [255, 255, 0, 35, 12, 0, 14, 255, 255, 99];
    let mut reader = Reader::new(&bytes);
    assert_eq!(slot::read(&mut reader).unwrap(), None);
    let value = slot::read(&mut reader).unwrap().unwrap();
    assert_eq!((value.id, value.count, value.damage), (35, 12, 14));
    assert_eq!(value.tag, Tag::End);
    assert_eq!(reader.u8().unwrap(), 99);
    let mut writer = Writer::new();
    slot::write(None, &mut writer).unwrap();
    slot::write(Some(&value), &mut writer).unwrap();
    assert_eq!(writer.data, bytes[..9]);
    for bytes in [
        &[255, 254][..],
        &[0, 1, 1, 0, 0, 255, 254],
        &[0, 1, 1, 0, 0, 0, 0],
    ] {
        assert!(slot::read(&mut Reader::new(bytes)).is_err());
    }
    let zero_count = [0, 1, 0, 0, 0, 255, 255, 99];
    let mut reader = Reader::new(&zero_count);
    assert_eq!(slot::read(&mut reader).unwrap(), None);
    assert_eq!(reader.u8().unwrap(), 99);
}

#[test]
fn gzip_named_nbt_slot_preserves_metadata_and_short_framing() {
    // Named compound root, TAG_String Name="Rust", TAG_End. Not a 1.8 NBT frame.
    let nbt = [
        10, 0, 0, 8, 0, 4, b'N', b'a', b'm', b'e', 0, 4, b'R', b'u', b's', b't', 0,
    ];
    let mut bytes = tagged_slot(&gzip(&nbt));
    bytes.push(99);
    let mut reader = Reader::new(&bytes);
    let value = slot::read(&mut reader).unwrap().unwrap();
    assert_eq!(
        value.tag,
        Tag::compound([("Name", Tag::String("Rust".into()))])
    );
    assert_eq!(reader.u8().unwrap(), 99);
    let mut writer = Writer::new();
    slot::write(Some(&value), &mut writer).unwrap();
    assert_eq!(&writer.data[..5], &[0, 35, 12, 0, 14]);
    let length = i16::from_be_bytes(writer.data[5..7].try_into().unwrap());
    assert_eq!(length as usize, writer.data.len() - 7);
    assert_eq!(&writer.data[7..9], &[0x1f, 0x8b]);
    assert_eq!(
        slot::read(&mut Reader::new(&writer.data)).unwrap(),
        Some(value)
    );
}

#[test]
fn legacy_item_compound_lists_are_not_interpreted_as_modern_mixed_lists() {
    let raw = [
        10, 0, 0, 9, 0, 1, b'l', 10, 0, 0, 0, 1, 3, 0, 0, 0, 0, 0, 7, 0, 0,
    ];
    let bytes = tagged_slot(&gzip(&raw));
    let item = slot::read(&mut Reader::new(&bytes)).unwrap().unwrap();
    assert_eq!(
        item.tag,
        Tag::compound([("l", Tag::List(vec![Tag::compound([("", Tag::Int(7))])]))]),
    );
    let mut writer = Writer::new();
    slot::write(Some(&item), &mut writer).unwrap();
    assert_eq!(
        slot::read(&mut Reader::new(&writer.data)).unwrap(),
        Some(item)
    );
    let decoded = horizon_world::compression::Compression::Gzip
        .decode(&writer.data[7..], 100)
        .unwrap();
    assert_eq!(decoded, raw);
}

#[test]
fn slots_reject_truncated_gzip_trailing_nbt_bombs_and_new_tag_types() {
    let empty_compound = [10, 0, 0, 0];
    let valid = gzip(&empty_compound);
    let mut trailing_gzip = valid.clone();
    trailing_gzip.push(0);
    let mut concatenated = valid.clone();
    concatenated.extend_from_slice(&valid);
    let mut checksum = valid.clone();
    let last = checksum.len() - 8;
    checksum[last] ^= 1;
    for compressed in [
        valid[..valid.len() - 1].to_vec(),
        trailing_gzip,
        concatenated,
        checksum,
        gzip(&[10, 0, 0, 0, 0]),
        gzip(&[3, 0, 0, 0, 0, 0, 1]),
        gzip(&vec![0; NBT_QUOTA + 1]),
    ] {
        assert!(slot::read(&mut Reader::new(&tagged_slot(&compressed))).is_err());
    }
    let root = nbt::NamedTag {
        name: "".into(),
        tag: Tag::compound([("new", Tag::LongArray(vec![1]))]),
    };
    let bytes = nbt::write_named(&root, nbt::Limits::default()).unwrap();
    assert!(slot::read(&mut Reader::new(&tagged_slot(&gzip(&bytes)))).is_err());
    let mut writer = Writer::new();
    writer.u8(99);
    let invalid = Slot {
        id: 1,
        count: 1,
        damage: 0,
        tag: root.tag,
    };
    assert!(slot::write(Some(&invalid), &mut writer).is_err());
    assert_eq!(writer.data, [99]);
    let mixed = Slot {
        tag: Tag::compound([("mixed", Tag::List(vec![Tag::Byte(1), Tag::Int(2)]))]),
        ..invalid
    };
    assert!(slot::write(Some(&mixed), &mut writer).is_err());
    assert_eq!(writer.data, [99]);
}

#[test]
fn incompressible_slot_nbt_cannot_wrap_the_signed_short_length() {
    let mut seed = 0x5634_1234u32;
    let noise: Vec<_> = (0..65_536)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as u8
        })
        .collect();
    let value = Slot {
        id: 1,
        count: 1,
        damage: 0,
        tag: Tag::compound([("data", Tag::ByteArray(noise))]),
    };
    let mut writer = Writer::new();
    writer.u8(99);
    assert!(slot::write(Some(&value), &mut writer).is_err());
    assert_eq!(writer.data, [99]);
}
