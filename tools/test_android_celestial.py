#!/usr/bin/env python3
"""Render actual celestial WGSL on a host GPU, without compiling the game."""
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def main():
    work = ROOT / '.local/android/celestial-tests'
    (work / 'src').mkdir(parents=True, exist_ok=True)
    (work / 'Cargo.toml').write_text('''[package]
name = "android-celestial-tests"
version = "0.0.0"
edition = "2024"
[workspace]
[dependencies]
wgpu = { version = "30.0.1", default-features = false, features = ["std", "parking_lot", "vulkan", "wgsl"] }
pollster = "0.4"
bytemuck = "1"
naga = { version = "30.0.1", features = ["wgsl-in"] }
glam = "0.30"
''', encoding='utf-8')
    gfx = (ROOT / 'crates/horizon-client-render/src/gfx.rs').read_text()
    entry = next(line for line in gfx.splitlines() if line.startswith('const CELESTIAL_VERTEX_ENTRY:'))
    shader = ROOT / 'crates/horizon-client-render/src/entity.wgsl'
    overlay = ROOT / 'crates/horizon-client-render/src/overlay.rs'
    tests = ROOT / 'crates/horizon-client-render/src/gfx/celestial_tests.rs'
    (work / 'src/lib.rs').write_text(f'''#![allow(dead_code)]
#[path = "{overlay}"] mod overlay;
mod gfx {{
{entry}
fn entity_shader() -> String {{ crate::overlay::wgsl_constants() + include_str!("{shader}") }}
#[cfg(test)] #[path = "{tests}"] mod celestial_tests;
}}
''', encoding='utf-8')
    subprocess.run(['cargo', 'test', '--manifest-path', str(work / 'Cargo.toml'), '--', '--nocapture'], cwd=ROOT, check=True)


if __name__ == '__main__':
    main()
