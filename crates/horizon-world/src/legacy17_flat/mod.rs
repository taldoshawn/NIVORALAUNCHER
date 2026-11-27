//! Isolated native 1.7 creative-flat terrain and saves. This deliberately
//! excludes normal generation, entities, scheduled/random ticks and survival.
mod chunk;
mod light;
use crate::{
    compression::Compression,
    nbt::{self, Limits, NamedTag, Tag},
    region::{ChunkRecord, RegionFile},
};
pub use chunk::Column;
use horizon_core::position::{BlockPos, ChunkPos};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicI64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub const GENERATOR_OPTIONS: &str = "2;7,2x3,2;1;";
const MARKER: &str = "nivora-legacy17-flat.json";
const LIMIT: usize = 512 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Version {
    #[serde(rename = "1.7.2")]
    V1_7_2,
    #[serde(rename = "1.7.10")]
    V1_7_10,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Marker {
    schema: u32,
    version: Version,
    preset: String,
}

pub struct World {
    directory: PathBuf,
    _lock: File,
    token: i64,
}

fn invalid(reason: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason.into())
}
fn limits() -> Limits {
    Limits {
        max_bytes: LIMIT,
        max_nodes: 8192,
        max_depth: 64,
    }
}
fn position(pos: ChunkPos) -> io::Result<()> {
    if !(-1_875_000..1_875_000).contains(&pos.x) || !(-1_875_000..1_875_000).contains(&pos.z) {
        return Err(invalid("legacy column outside the supported world bounds"));
    }
    Ok(())
}
fn supported(id: u16, metadata: u8) -> bool {
    metadata == 0 && matches!(id, 0 | 1 | 2 | 3 | 4 | 5 | 7 | 20 | 22 | 24 | 42 | 45)
}
fn bounded_file(path: &Path, limit: usize) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(invalid("legacy file exceeds its size limit"));
    }
    Ok(bytes)
}
fn decode(bytes: &[u8], compression: Compression) -> io::Result<NamedTag> {
    if bytes.len() > LIMIT {
        return Err(invalid("oversized legacy compressed NBT"));
    }
    let mut output = Vec::new();
    let trailing = match compression {
        Compression::Gzip => {
            let mut decoder = flate2::bufread::GzDecoder::new(bytes);
            (&mut decoder)
                .take(LIMIT as u64 + 1)
                .read_to_end(&mut output)?;
            !decoder.get_ref().is_empty()
        }
        Compression::Zlib => {
            let mut decoder = flate2::bufread::ZlibDecoder::new(bytes);
            (&mut decoder)
                .take(LIMIT as u64 + 1)
                .read_to_end(&mut output)?;
            !decoder.get_ref().is_empty()
        }
        _ => return Err(invalid("unsupported 1.7 NBT compression")),
    };
    if trailing || output.len() > LIMIT {
        return Err(invalid("oversized or trailing legacy compressed NBT"));
    }
    let (root, used) =
        nbt::read_named_legacy(&output, limits()).map_err(|e| invalid(e.to_string()))?;
    if used != output.len() {
        return Err(invalid("trailing legacy NBT"));
    }
    Ok(root)
}
fn encode(root: &NamedTag, compression: Compression) -> io::Result<Vec<u8>> {
    let bytes = nbt::write_named_legacy(root, limits()).map_err(|e| invalid(e.to_string()))?;
    compression.encode(&bytes)
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
fn acquire(directory: &Path) -> io::Result<(File, i64)> {
    static TOKEN: AtomicI64 = AtomicI64::new(0);
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("session.lock"))?;
    file.try_lock()
        .map_err(|e| io::Error::other(format!("legacy world is already open: {e}")))?;
    let clock = now();
    let token = TOKEN
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |old| {
            Some(clock.max(old.saturating_add(1)))
        })
        .unwrap();
    let token = clock.max(token.saturating_add(1));
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&token.to_be_bytes())?;
    file.set_len(8)?;
    file.sync_all()?;
    Ok((file, token))
}

fn level(name: &str) -> NamedTag {
    NamedTag {
        name: Default::default(),
        tag: Tag::compound([(
            "Data",
            Tag::compound([
                ("version", Tag::Int(19133)),
                ("LevelName", Tag::String(name.into())),
                ("generatorName", Tag::String("flat".into())),
                ("generatorVersion", Tag::Int(0)),
                ("generatorOptions", Tag::String(GENERATOR_OPTIONS.into())),
                ("RandomSeed", Tag::Long(0)),
                ("MapFeatures", Tag::Byte(0)),
                ("GameType", Tag::Int(1)),
                ("hardcore", Tag::Byte(0)),
                ("allowCommands", Tag::Byte(1)),
                ("initialized", Tag::Byte(1)),
                ("SpawnX", Tag::Int(0)),
                ("SpawnY", Tag::Int(4)),
                ("SpawnZ", Tag::Int(0)),
                ("Time", Tag::Long(6000)),
                ("DayTime", Tag::Long(6000)),
                ("LastPlayed", Tag::Long(now())),
                ("SizeOnDisk", Tag::Long(0)),
                ("raining", Tag::Byte(0)),
                ("rainTime", Tag::Int(0)),
                ("thundering", Tag::Byte(0)),
                ("thunderTime", Tag::Int(0)),
                (
                    "GameRules",
                    Tag::compound([
                        ("doMobSpawning", Tag::String("false".into())),
                        ("doDaylightCycle", Tag::String("false".into())),
                    ]),
                ),
            ]),
        )]),
    }
}
fn validate_level(root: &NamedTag) -> io::Result<()> {
    let data = root
        .tag
        .get("Data")
        .ok_or_else(|| invalid("missing legacy level Data"))?;
    if root.tag.get("DataVersion").is_some()
        || data.get("DataVersion").is_some()
        || data.get("version") != Some(&Tag::Int(19133))
        || data.get("generatorName") != Some(&Tag::String("flat".into()))
        || data.get("generatorVersion") != Some(&Tag::Int(0))
        || data.get("generatorOptions") != Some(&Tag::String(GENERATOR_OPTIONS.into()))
        || data.get("GameType") != Some(&Tag::Int(1))
        || data.get("MapFeatures") != Some(&Tag::Byte(0))
        || data.get("hardcore") != Some(&Tag::Byte(0))
    {
        return Err(invalid("foreign or unsupported legacy flat level.dat"));
    }
    Ok(())
}

impl World {
    /// Creates only an absent/empty directory. Existing saves are never adopted.
    pub fn create(directory: impl AsRef<Path>, version: Version, name: &str) -> io::Result<Self> {
        let directory = directory.as_ref().to_owned();
        if name.len() > 256 {
            return Err(invalid("legacy world name is too long"));
        }
        if directory.exists() && fs::read_dir(&directory)?.next().is_some() {
            return Err(invalid("refusing to replace an existing world directory"));
        }
        fs::create_dir_all(&directory)?;
        let (lock, token) = acquire(&directory)?;
        // Recheck after locking: another creator may have won the race.
        if fs::read_dir(&directory)?.any(|e| !e.is_ok_and(|e| e.file_name() == "session.lock")) {
            return Err(invalid("world directory changed while creating it"));
        }
        let marker = Marker {
            schema: 1,
            version,
            preset: GENERATOR_OPTIONS.into(),
        };
        let marker = serde_json::to_vec_pretty(&marker).map_err(|e| invalid(e.to_string()))?;
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(MARKER))?;
        f.write_all(&marker)?;
        f.sync_all()?;
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("level.dat"))?;
        f.write_all(&encode(&level(name), Compression::Gzip)?)?;
        f.sync_all()?;
        Ok(Self {
            directory,
            _lock: lock,
            token,
        })
    }

    pub fn open(directory: impl AsRef<Path>, version: Version) -> io::Result<Self> {
        let directory = directory.as_ref().to_owned();
        let marker: Marker = serde_json::from_slice(&bounded_file(&directory.join(MARKER), 4096)?)
            .map_err(|e| invalid(e.to_string()))?;
        if marker.schema != 1 || marker.version != version || marker.preset != GENERATOR_OPTIONS {
            return Err(invalid("world belongs to another version or generator"));
        }
        validate_level(&decode(
            &bounded_file(&directory.join("level.dat"), LIMIT)?,
            Compression::Gzip,
        )?)?;
        let (lock, token) = acquire(&directory)?;
        Ok(Self {
            directory,
            _lock: lock,
            token,
        })
    }

    /// 1.7 uses a timestamp in session.lock. Keep an OS lock too, and detect
    /// another writer replacing the timestamp before any terrain/player write.
    pub fn check_session(&self) -> io::Result<()> {
        if bounded_file(&self.directory.join("session.lock"), 8)? != self.token.to_be_bytes() {
            return Err(invalid("legacy world session.lock changed"));
        }
        Ok(())
    }
    fn region_path(&self, pos: ChunkPos) -> PathBuf {
        let p = pos.region();
        self.directory
            .join("region")
            .join(format!("r.{}.{}.mca", p.x, p.z))
    }
    pub fn column(&self, pos: ChunkPos) -> io::Result<Column> {
        position(pos)?;
        self.check_session()?;
        let path = self.region_path(pos);
        if !path.exists() {
            return Column::classic(pos);
        }
        let mut region = RegionFile::open_strict(path, false)?;
        let Some(record) = region.read(pos)? else {
            return Column::classic(pos);
        };
        Column::from_nbt(decode(&record.compressed, record.compression)?, pos)
    }
    fn store(&self, column: Column) -> io::Result<()> {
        self.check_session()?;
        let pos = column.pos;
        let record = ChunkRecord {
            compression: Compression::Zlib,
            compressed: encode(&column.into_nbt(), Compression::Zlib)?,
            timestamp: (now() / 1000) as u32,
        };
        let path = self.region_path(pos);
        fs::create_dir_all(path.parent().unwrap())?;
        let mut region = if path.exists() {
            RegionFile::open_strict(path, true)?
        } else {
            RegionFile::open_writable(path, true)?
        };
        region.write(pos, &record)?;
        region.sync()
    }
    pub fn generate(&mut self, center: ChunkPos, radius: i32) -> io::Result<()> {
        if !(0..=4).contains(&radius) {
            return Err(invalid("legacy generation radius exceeds four"));
        }
        position(center)?;
        position(ChunkPos::new(center.x - radius, center.z - radius))?;
        position(ChunkPos::new(center.x + radius, center.z + radius))?;
        let mut columns = Vec::new();
        for z in -radius..=radius {
            for x in -radius..=radius {
                columns.push(self.column(ChunkPos::new(center.x + x, center.z + z))?);
            }
        }
        for column in columns {
            self.store(column)?;
        }
        Ok(())
    }
    /// Bounded terrain edits in one column. Player reach/inventory/collision
    /// authorization belongs to the protocol server, not this save primitive.
    pub fn edit(&mut self, edits: &[(BlockPos, u16, u8)]) -> io::Result<usize> {
        if edits.len() > 4096 {
            return Err(invalid("too many legacy edits"));
        }
        let Some((first, _, _)) = edits.first() else {
            return Ok(0);
        };
        let center = first.chunk();
        position(center)?;
        position(ChunkPos::new(center.x - 2, center.z - 2))?;
        position(ChunkPos::new(center.x + 2, center.z + 2))?;
        if edits.iter().any(|(p, id, meta)| {
            p.chunk() != center || !(0..256).contains(&p.y) || !supported(*id, *meta)
        }) {
            return Err(invalid("unsupported or cross-column legacy edit"));
        }
        let mut columns = Vec::with_capacity(25);
        for z in -2..=2 {
            for x in -2..=2 {
                columns.push(self.column(ChunkPos::new(center.x + x, center.z + z))?);
            }
        }
        let before = columns.clone();
        let mut changed = 0;
        for &(p, id, _) in edits {
            changed += usize::from(columns[12].set(p, id as u8)?);
        }
        if changed == 0 {
            return Ok(0);
        }
        light::relight(&mut columns);
        for (i, column) in columns.into_iter().enumerate() {
            if (1..=3).contains(&(i % 5)) && (1..=3).contains(&(i / 5)) && column != before[i] {
                self.store(column)?;
            }
        }
        Ok(changed)
    }
}
