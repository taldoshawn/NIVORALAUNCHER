use horizon_core::position::{BlockPos, ChunkPos};
use horizon_world::{
    compression::Compression,
    legacy17_flat::{Column, GENERATOR_OPTIONS, Version, World},
    nbt::{self, Limits, Tag},
    region::{ChunkRecord, RegionFile},
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "nivora-world17-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn pos(x: i32, y: i32, z: i32) -> BlockPos {
    BlockPos::new(x, y, z)
}
fn world(t: &Temp) -> World {
    World::create(&t.0, Version::V1_7_10, "Fixture").unwrap()
}

#[test]
fn classic_layers_are_native_legacy_arrays_and_negative_coordinates_survive_disk() {
    for version in [Version::V1_7_2, Version::V1_7_10] {
        let t = Temp::new();
        let mut w = World::create(&t.0, version, "Fixture").unwrap();
        w.generate(ChunkPos::new(-1, -1), 0).unwrap();
        let c = w.column(ChunkPos::new(-1, -1)).unwrap();
        for y in 0..256 {
            let id = match y {
                0 => 7,
                1 | 2 => 3,
                3 => 2,
                _ => 0,
            };
            for z in -16..0 {
                for x in -16..0 {
                    assert_eq!(c.block(pos(x, y, z)).unwrap(), id);
                    assert_eq!(
                        c.skylight(pos(x, y, z)).unwrap(),
                        if y < 4 { 0 } else { 15 }
                    );
                }
            }
        }
        let root = c.into_nbt();
        assert!(root.tag.get("DataVersion").is_none());
        let level = root.tag.get("Level").unwrap();
        assert_eq!(level.get("HeightMap"), Some(&Tag::IntArray(vec![4; 256])));
        assert_eq!(level.get("Biomes"), Some(&Tag::ByteArray(vec![1; 256])));
        let raw = Compression::Gzip
            .decode(&fs::read(t.0.join("level.dat")).unwrap(), 65536)
            .unwrap();
        let (level, used) = nbt::read_named_legacy(&raw, Limits::default()).unwrap();
        assert_eq!(used, raw.len());
        let data = level.tag.get("Data").unwrap();
        assert!(data.get("DataVersion").is_none());
        assert_eq!(data.get("version"), Some(&Tag::Int(19133)));
        assert_eq!(
            data.get("generatorOptions"),
            Some(&Tag::String(GENERATOR_OPTIONS.into()))
        );
        drop(w);
        let w = World::open(&t.0, version).unwrap();
        assert_eq!(
            w.column(ChunkPos::new(-1, -1))
                .unwrap()
                .block(pos(-1, 3, -1))
                .unwrap(),
            2
        );
    }
}

#[test]
fn roof_and_glass_relight_across_chunk_edges_and_removal_restores_sky() {
    let t = Temp::new();
    let mut w = world(&t);
    w.generate(ChunkPos::new(0, 0), 1).unwrap();
    for (a, b) in [(14, 15), (16, 16)] {
        let edits: Vec<_> = (0..=2)
            .flat_map(|z| (a..=b).map(move |x| (pos(x, 8, z), 1, 0)))
            .collect();
        w.edit(&edits).unwrap();
    }
    let c = w.column(ChunkPos::new(0, 0)).unwrap();
    assert_eq!(c.height(15, 1), Some(9));
    assert_eq!(c.skylight(pos(15, 7, 1)).unwrap(), 13);
    assert_eq!(
        w.column(ChunkPos::new(1, 0))
            .unwrap()
            .skylight(pos(16, 7, 1))
            .unwrap(),
        14
    );
    w.edit(&[(pos(15, 8, 1), 20, 0)]).unwrap();
    let c = w.column(ChunkPos::new(0, 0)).unwrap();
    assert_eq!(c.block(pos(15, 8, 1)).unwrap(), 20);
    assert_eq!(c.height(15, 1), Some(4));
    assert_eq!(c.skylight(pos(15, 7, 1)).unwrap(), 15);
    for (a, b) in [(14, 15), (16, 16)] {
        let edits: Vec<_> = (0..=2)
            .flat_map(|z| (a..=b).map(move |x| (pos(x, 8, z), 0, 0)))
            .collect();
        w.edit(&edits).unwrap();
    }
    drop(w);
    let w = World::open(&t.0, Version::V1_7_10).unwrap();
    for x in 14..=16 {
        let p = pos(x, 7, 1);
        let c = w.column(p.chunk()).unwrap();
        assert_eq!(c.skylight(p).unwrap(), 15);
        assert_eq!(c.height((x & 15) as u8, 1), Some(4));
    }
}

#[test]
fn historical_highest_section_top_layer_is_not_a_modern_heightmap() {
    let t = Temp::new();
    let mut w = world(&t);
    for (x, y) in [(0, 31), (16, 255)] {
        w.edit(&[(pos(x, y, 0), 1, 0)]).unwrap();
        let c = w.column(pos(x, y, 0).chunk()).unwrap();
        assert_eq!(c.height(0, 0), Some(4));
        assert_eq!(c.block(pos(x, y, 0)).unwrap(), 1);
        assert_eq!(c.skylight(pos(x, y - 1, 0)).unwrap(), 15);
    }
    w.edit(&[(pos(0, 30, 0), 1, 0)]).unwrap();
    assert_eq!(
        w.column(ChunkPos::new(0, 0)).unwrap().height(0, 0),
        Some(31)
    );
    w.edit(&[(pos(0, 30, 0), 0, 0)]).unwrap();
    assert_eq!(w.column(ChunkPos::new(0, 0)).unwrap().height(0, 0), Some(4));
}

#[test]
fn version_ownership_and_session_lock_prevent_foreign_or_concurrent_writes() {
    let t = Temp::new();
    let w = world(&t);
    let token = fs::read(t.0.join("session.lock")).unwrap();
    assert!(World::open(&t.0, Version::V1_7_10).is_err());
    assert!(World::open(&t.0, Version::V1_7_2).is_err());
    assert!(World::create(&t.0, Version::V1_7_10, "overwrite").is_err());
    assert_eq!(fs::read(t.0.join("session.lock")).unwrap(), token);
    w.check_session().unwrap();
    fs::write(t.0.join("session.lock"), 123i64.to_be_bytes()).unwrap();
    assert!(w.check_session().is_err());
    assert!(w.column(ChunkPos::new(0, 0)).is_err());
    drop(w);
    let _w = World::open(&t.0, Version::V1_7_10).unwrap();
    let other = Temp::new();
    fs::write(other.0.join("level.dat"), b"foreign save").unwrap();
    assert!(World::create(&other.0, Version::V1_7_10, "overwrite").is_err());
    assert!(World::open(&other.0, Version::V1_7_10).is_err());
    assert_eq!(
        fs::read(other.0.join("level.dat")).unwrap(),
        b"foreign save"
    );
    assert!(!other.0.join("session.lock").exists());
}

#[test]
fn malformed_arrays_regions_and_compression_fail_without_replacing_the_input() {
    let t = Temp::new();
    let mut w = world(&t);
    w.generate(ChunkPos::new(0, 0), 0).unwrap();
    let path = t.0.join("region/r.0.0.mca");
    let original = fs::read(&path).unwrap();
    for edits in [
        vec![(pos(0, 4, 0), 10, 0)],
        vec![(pos(0, 4, 0), 1, 1)],
        vec![(pos(0, 256, 0), 1, 0)],
        vec![(pos(0, 4, 0), 1, 0), (pos(16, 4, 0), 1, 0)],
        vec![(pos(0, 4, 0), 1, 0); 4097],
    ] {
        assert!(w.edit(&edits).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
    }
    assert!(w.generate(ChunkPos::new(0, 0), 5).is_err());
    let mut root = Column::classic(ChunkPos::new(0, 0)).unwrap().into_nbt();
    let Some(Tag::List(sections)) = root.tag.get_mut("Level").unwrap().get_mut("Sections") else {
        panic!()
    };
    *sections[0].get_mut("Blocks").unwrap() = Tag::ByteArray(vec![1]);
    let encoded = nbt::write_named_legacy(&root, Limits::default()).unwrap();
    let mut region = RegionFile::open_strict(&path, true).unwrap();
    region
        .write(
            ChunkPos::new(0, 0),
            &ChunkRecord {
                compression: Compression::Zlib,
                compressed: Compression::Zlib.encode(&encoded).unwrap(),
                timestamp: 0,
            },
        )
        .unwrap();
    region.sync().unwrap();
    drop(region);
    let malformed = fs::read(&path).unwrap();
    assert!(w.column(ChunkPos::new(0, 0)).is_err());
    assert!(w.edit(&[(pos(0, 4, 0), 1, 0)]).is_err());
    assert_eq!(fs::read(&path).unwrap(), malformed);
    fs::write(&path, b"truncated region").unwrap();
    assert!(w.column(ChunkPos::new(0, 0)).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"truncated region");
    drop(w);
    let level = t.0.join("level.dat");
    let valid = fs::read(&level).unwrap();
    let mut trailing = valid.clone();
    trailing.push(0);
    for bytes in [
        valid[..valid.len() - 1].to_vec(),
        trailing,
        Compression::Gzip.encode(&vec![0; 512 * 1024 + 1]).unwrap(),
    ] {
        fs::write(&level, &bytes).unwrap();
        assert!(World::open(&t.0, Version::V1_7_10).is_err());
        assert_eq!(fs::read(&level).unwrap(), bytes);
    }
}
