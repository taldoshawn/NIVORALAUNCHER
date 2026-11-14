//! The network adapter must resolve every numeric state before a public server
//! can send it. Run against the same generated registry packaged in the APK.
use horizon_protocol::legacy18::{chunk::decode_bulk, play::Packet};
use horizon_world::{datafix::legacy_block_state, registry::BlockRegistry};

#[test]
#[ignore = "requires private prepared facts.json via HORIZON_LEGACY_FACTS"]
fn all_protocol47_states_resolve_in_the_packaged_registry() {
    let path = std::env::var_os("HORIZON_LEGACY_FACTS")
        .expect("set HORIZON_LEGACY_FACTS to the prepared facts.json");
    let registry = BlockRegistry::from_facts_json(std::fs::File::open(path).unwrap()).unwrap();
    for sky in [false, true] {
        // Independent wire fixture: one bulk column, section y=0, all 4096
        // legacy states in x/z/y order, followed by light and biome arrays.
        let mut bytes = vec![u8::from(sky), 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
        for state in 0u16..4096 {
            bytes.extend_from_slice(&state.to_le_bytes());
        }
        bytes.resize(bytes.len() + 2048 + if sky { 2048 } else { 0 } + 256, 0);
        let Packet::ChunkBulk(payload) = Packet::decode(0x26, &bytes).unwrap() else {
            panic!("0x26 must be a bulk chunk");
        };
        let columns = decode_bulk(payload).unwrap();
        assert_eq!(columns.len(), 1);
        let section = columns[0].section(0).unwrap();
        let mut failures = Vec::new();
        for numeric in 0u16..4096 {
            let (x, y, z) = (
                (numeric & 15) as u8,
                (numeric >> 8) as u8,
                ((numeric >> 4) & 15) as u8,
            );
            let decoded = section.state(x, y, z).unwrap();
            assert_eq!(decoded, numeric);
            let descriptor = legacy_block_state(decoded).unwrap();
            if let Err(error) = registry.resolve(&descriptor) {
                failures.push(format!(
                    "{numeric:#05x} ({}/{}) {descriptor:?}: {error}",
                    numeric >> 4,
                    numeric & 15
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "{} unresolvable states:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}
