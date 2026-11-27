#!/usr/bin/env python3
"""Exercise classic shader search/install code without GPUI, an APK or player data.

Uses the shared browser's data/policy and manifest sections verbatim. Only the
unrelated registry/Java backends and proxy configuration are replaced by constants
or an environment-proxy fixture; Modrinth, downloads and archive validation are real modules.
Native shader catalog generation/activation is outside this isolated harness.
"""
from pathlib import Path
import argparse
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--live', action='store_true', help='also query live Modrinth metadata')
    args = parser.parse_args()
    harness = ROOT / '.local/android/shader-tests'
    (harness / 'src').mkdir(parents=True, exist_ok=True)
    (harness / 'Cargo.toml').write_text('''[package]
name = "android-shader-tests"
version = "0.0.0"
edition = "2024"
[workspace]
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha1 = "0.10"
sha2 = "0.10"
ureq = { version = "2", default-features = false, features = ["tls", "json", "native-certs", "proxy-from-env"] }
zip = { version = "9", default-features = false, features = ["deflate"] }
horizon-mods = { path = "../../../crates/horizon-mods", default-features = false, features = ["index"] }
horizon-port-pack = { path = "../../../crates/horizon-port-pack" }
''', encoding='utf-8')
    lock = tomllib.loads((ROOT / 'Cargo.lock').read_text(encoding='utf-8'))
    zip_version = next(p['version'] for p in lock['package']
                       if p['name'] == 'zip' and p['version'].startswith('9.'))
    cargo = harness / 'Cargo.toml'
    cargo.write_text(cargo.read_text().replace('version = "9"', f'version = "={zip_version}"'))
    browser = (ROOT / 'launcher/src/browser.rs').read_text(encoding='utf-8')
    browser = browser[browser.index('use serde::'):browser.index('#[cfg(test)]')]
    start = browser.index('/// The sources this launcher uses:')
    end = browser.index('/// How well a project', start)
    browser = browser[:start] + browser[end:]
    manifest = (ROOT / 'launcher/src/browser/installed.rs').read_text(encoding='utf-8')
    manifest = manifest[manifest.index('const MANIFEST:'):manifest.index('/// One file to install:')]

    def module(name, path):
        return f'#[path = "{(ROOT / path).as_posix()}"] pub mod {name};\n'

    source = '''#![allow(dead_code, unused_imports)]
extern crate self as horizon_client;
pub mod net {
    pub fn keep<T>(_: &str, _: Option<&str>, make: impl FnOnce(Option<ureq::Proxy>) -> T) -> T { make(None) }
}
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
mod backend { pub const VERSION: &str = "shader-test"; }
mod proxy { pub fn spec_for(_: &str) -> Option<String> { None } }
mod i18n { pub fn tr(text: &str) -> &str { text } }
'''
    source += module('content_files', 'crates/horizon-client/src/content_files.rs')
    source += 'pub mod browser {\n' + browser
    source += module('details', 'launcher/src/browser/details.rs')
    source += module('download', 'launcher/src/browser/download.rs')
    source += module('modrinth', 'launcher/src/browser/modrinth.rs')
    source += module('lock', 'launcher/src/browser/lock.rs')
    source += 'pub use lock::lock_home;\n'
    for name, path, constant in [('horizon', 'launcher/src/browser/horizon.rs', 'LOADER'),
                                 ('java', 'launcher/src/browser/java.rs', 'LOADERS')]:
        line = next(line for line in (ROOT / path).read_text(encoding='utf-8').splitlines()
                    if line.startswith(f'pub const {constant}:'))
        extra = ('pub fn is_official(_: &str) -> bool { false }' if name == 'horizon'
                 else 'pub const MODS_DIR: &str = "horizon-mods";')
        source += f'pub mod {name} {{ {line} {extra} }}\n'
    source += 'pub mod installed { use super::*; use std::{fs, path::{Component, Path, PathBuf}};\n' + manifest + '\n}\n}\n'
    shaders = (ROOT / 'crates/horizon-client/src/android/shader_catalog.rs').read_text()
    native = shaders.index('/// Returns all built-in Horizon Native Shader Packs')
    tests = shaders.index('#[cfg(test)]', native)
    source += 'mod shader_catalog {\n' + shaders[:native] + shaders[tests:] + '\n}\n'
    source += '''#[test] #[ignore = "live Modrinth metadata, no downloads"]
fn live_shader_search() {
    let mut q = browser::Query::new("", browser::Kind::Shader);
    q.everything = true; q.limit = 3;
    let hits = browser::modrinth::search_shader_packs(&q).unwrap();
    assert!(!hits.is_empty());
    for p in hits { assert_eq!(p.kind, browser::Kind::Shader); println!("{} {}", p.id, p.title); }
}
#[test] #[ignore = "live Modrinth metadata, no downloads"]
fn live_mod_search() {
    use browser::Source;
    let mut q = browser::Query::new("", browser::Kind::Mod);
    q.everything = true; q.limit = 3;
    let hits = browser::modrinth::Modrinth.search(&q).unwrap();
    assert!(!hits.is_empty());
    for p in hits { assert_eq!(p.kind, browser::Kind::Mod); println!("{} {}", p.id, p.title); }
}
'''
    (harness / 'src/lib.rs').write_text(source, encoding='utf-8')
    command = ['cargo', 'test', '--manifest-path', str(harness / 'Cargo.toml')]
    subprocess.run(command, cwd=ROOT, check=True)
    if args.live:
        subprocess.run(command + ['live_', '--', '--ignored', '--nocapture'],
                       cwd=ROOT, check=True)


if __name__ == '__main__':
    main()
