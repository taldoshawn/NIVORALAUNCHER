//! Disposable native 1.7 save fixture for tools/test_legacy17_flat.py.
use horizon_core::position::{BlockPos, ChunkPos};
use horizon_world::legacy17_flat::{GENERATOR_OPTIONS, Version, World};
use std::{collections::BTreeMap, io, path::PathBuf};

fn fixture() -> Vec<(BlockPos, u16, u8)> {
    let mut edits = Vec::new();
    for z in -1..=1 {
        for x in 14..=18 {
            edits.push((BlockPos::new(x, 8, z), 1, 0));
        }
    }
    edits.extend([
        (BlockPos::new(3, 8, 3), 20, 0),
        (BlockPos::new(-5, 40, 4), 1, 0),
        (BlockPos::new(6, 255, 5), 1, 0),
        (BlockPos::new(-8, 31, -8), 1, 0),
    ]);
    edits
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("usage: legacy17_flat_fixture VERSION generate|edit|remove DIRECTORY".into());
    }
    let version = match args[1].as_str() {
        "1.7.2" => Version::V1_7_2,
        "1.7.10" => Version::V1_7_10,
        _ => return Err("unsupported fixture version".into()),
    };
    let directory = PathBuf::from(&args[3]);
    match args[2].as_str() {
        "generate" => {
            let mut world = World::create(directory, version, "Native 1.7 fixture")?;
            world.generate(ChunkPos::new(0, 0), 1)?;
        }
        stage @ ("edit" | "remove") => {
            let mut world = World::open(directory, version)?;
            let mut by_column: BTreeMap<(i32, i32), Vec<_>> = BTreeMap::new();
            for (pos, id, metadata) in fixture() {
                let chunk = pos.chunk();
                by_column.entry((chunk.x, chunk.z)).or_default().push((
                    pos,
                    if stage == "remove" { 0 } else { id },
                    metadata,
                ));
            }
            for edits in by_column.values() {
                world.edit(edits)?;
            }
        }
        _ => {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "unknown fixture stage").into());
        }
    }
    println!(
        "{}",
        serde_json::json!({"version": args[1], "stage": args[2], "preset": GENERATOR_OPTIONS, "columns": 9})
    );
    Ok(())
}
