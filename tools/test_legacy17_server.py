#!/usr/bin/env python3
"""Exercise native Rust 1.7 networking against a disposable vanilla server.

Java is a development oracle, never a player-facing game backend. Official
inputs stay under ignored .local/. The server binds only to loopback and uses
a fresh world/player. This checks saved server position and observed block
updates, presentation packets and saved sign text. It does not test rendering,
audio output, native world generation, authentication or Android.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import urllib.request
import uuid

import anvil_dump
import datafix_worlds
import lab

ROOT = Path(__file__).resolve().parent.parent
NAME = "Legacy17Probe"
VERSIONS = {"1.7.2": 4, "1.7.10": 5}


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def checked_server(version):
    # Reuse the existing verified version-metadata downloader and input cache.
    # Its DataVersion table starts at 1.7.10, so fetch 1.7.2's jar from the same
    # metadata here and verify both newly downloaded and cached bytes.
    reference = datafix_worlds.version_meta(version)["downloads"]["server"]
    jar = datafix_worlds.SHARED / "servers" / version / "server.jar"
    if jar.is_file():
        data = jar.read_bytes()
    else:
        with urllib.request.urlopen(reference["url"], timeout=60) as response:
            data = response.read(reference["size"] + 1)
    if hashlib.sha1(data).hexdigest() != reference["sha1"] or len(data) != reference["size"]:
        raise RuntimeError("cached vanilla server size/checksum mismatch")
    if not jar.exists():
        jar.parent.mkdir(parents=True, exist_ok=True)
        jar.write_bytes(data)
    return jar, reference["sha1"]


def isolated_environment(root):
    env = os.environ.copy()
    for variable in ("HORIZON_HOME", "HORIZON_LAUNCHER_HOME", "XDG_DATA_HOME", "XDG_CONFIG_HOME",
                     "HORIZON_LAUNCHER_TRASH", "HORIZON_LAUNCHER_CLIPBOARD", "HORIZON_LAUNCHER_DESKTOP"):
        directory = root / variable.lower()
        directory.mkdir()
        env[variable] = str(directory)
    env["HORIZON_LAUNCHER_NO_SELF_UPDATE"] = "1"
    env["HORIZON_NO_AUDIO"] = "1"
    return env


def run(args):
    jar, sha1 = checked_server(args.version)
    if not args.skip_build:
        build_env = os.environ.copy()
        build_env.setdefault("CARGO_TARGET_DIR", str(ROOT / ".local" / "parity-target"))
        subprocess.run(["cargo", "build", "--locked", "-j", "2", "-p", "horizon-protocol",
                        "--example", "legacy17_server_probe"], cwd=ROOT, env=build_env, check=True)
        probe = ROOT / build_env["CARGO_TARGET_DIR"] / "debug" / "examples" / "legacy17_server_probe"
    else:
        probe = lab.cargo_target() / "debug" / "examples" / "legacy17_server_probe"
    if args.probe:
        probe = args.probe.resolve()
    if not probe.is_file():
        raise RuntimeError(f"probe executable does not exist: {probe}")
    evidence_dir = (args.evidence or ROOT / ".local" / "legacy17-evidence" / args.version).resolve()
    evidence_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="nivora-legacy17-") as temporary:
        root = Path(temporary)
        env = isolated_environment(root)
        server_dir = root / "server"
        server_dir.mkdir()
        offline_id = str(uuid.UUID(bytes=hashlib.md5(("OfflinePlayer:" + NAME).encode()).digest(), version=3))
        if args.version == "1.7.2":
            (server_dir / "ops.txt").write_text(NAME + "\n", encoding="utf-8")
        else:
            (server_dir / "ops.json").write_text(json.dumps([{
                "uuid": offline_id, "name": NAME, "level": 4, "bypassesPlayerLimit": True,
            }]), encoding="utf-8")
        for name in ("banned-players.json", "banned-ips.json", "whitelist.json"):
            (server_dir / name).write_text("[]", encoding="utf-8")
        port = free_port()
        server = None
        previous = os.environ.copy()
        try:
            os.environ.update(env)
            properties = {
                "server-ip": "127.0.0.1", "level-type": "FLAT", "generator-settings": "2;7,62x3,2;1;",
                "gamemode": "1", "difficulty": "0", "max-players": "2", "generate-structures": "false",
                "spawn-animals": "false", "spawn-monsters": "false", "allow-flight": "true",
            }
            server = datafix_worlds.Server(server_dir, jar, port, java=args.java, props_override=properties)
            # 1.7 refuses /setblock in unloaded chunks. Persist our desired spawn
            # so startup prepares its region before creating the fixture.
            server.run("setworldspawn 0 64 0")
            server.stop()
            server = datafix_worlds.Server(server_dir, jar, port, java=args.java, props_override=properties)
            for command in ("gamerule doMobSpawning false", "gamerule doDaylightCycle false",
                            "setworldspawn 0 64 0", "time set day", "setblock 0 65 3 stone",
                            "setblock 6 65 0 wool 14"):
                lines = server.run(command)
                if datafix_worlds.failed(lines) or any("Cannot " in line for line in lines):
                    raise RuntimeError(f"fixture command failed: {command}: {lines}")
            result = subprocess.run([str(probe), f"127.0.0.1:{port}", "--version", args.version], cwd=ROOT, env=env,
                                    capture_output=True, text=True, timeout=65)
            (evidence_dir / "probe-stdout.txt").write_text(result.stdout, encoding="utf-8")
            (evidence_dir / "probe-stderr.txt").write_text(result.stderr, encoding="utf-8")
            if result.returncode:
                raise RuntimeError(f"Rust probe exited {result.returncode}: {result.stderr.strip()}")
            report = json.loads(result.stdout.strip().splitlines()[-1])
            if report.get("protocol") != VERSIONS[args.version] or report.get("profile") != NAME:
                raise RuntimeError(f"wrong reference protocol/profile: {report}")
            required = ("joined", "position_acknowledged", "fixture_teleport", "stone_anchor",
                        "red_wool_metadata", "placement_confirmed", "break_confirmed",
                        "server_brand", "named_sound", "collected_entity", "attached_entity",
                        "sign_editor", "sign_text", "map_column", "map_scale", "map_player_icon_rotation")
            if not all(report.get(key) is True for key in required):
                raise RuntimeError(f"missing server interaction evidence: {report}")
            if report["columns"] < 4 or report["keepalives"] < 2:
                raise RuntimeError("insufficient chunk/keepalive evidence")
            server.wait("lost connection", timeout=10)
            server.run("save-all")
            server.stop()
            player = (Path("players") / f"{NAME}.dat" if args.version == "1.7.2"
                      else Path("playerdata") / f"{offline_id}.dat")
            saved = anvil_dump.read_dat(server_dir / "world" / player)
            position = [coordinate[1] for coordinate in saved["Pos"]]
            if len(position) != 3 or any(abs(actual - expected) > 1e-6
                                         for actual, expected in zip(position, (1.5, 64.0, 0.5))):
                raise RuntimeError(f"server did not accept the movement/stance: saved {position}")
            chunk = anvil_dump.region_chunks(server_dir / "world" / "region" / "r.0.0.mca")[(0, 0)]["Level"]
            signs = [tag for tag in chunk.get("TileEntities", [])
                     if tag.get("id") == "Sign" and (tag.get("x"), tag.get("y"), tag.get("z")) == (2, 64, 1)]
            expected_lines = ["Nivora", "Rust17", "", "Native"]
            if len(signs) != 1 or [signs[0].get(f"Text{i}") for i in range(1, 5)] != expected_lines:
                raise RuntimeError(f"server did not persist the native sign edit: {signs}")
            report.update({"server_version": args.version, "server_sha1": sha1,
                           "server_saved_position": position, "server_movement_confirmed": True,
                           "server_saved_sign": expected_lines, "server_sign_edit_confirmed": True})
            (evidence_dir / "result.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
            print(json.dumps(report, indent=2))
        finally:
            if server is not None:
                if server.process.poll() is None:
                    server.stop()
                (evidence_dir / "server.log").write_text("\n".join(server.lines) + "\n", encoding="utf-8")
            os.environ.clear()
            os.environ.update(previous)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", choices=VERSIONS, default="1.7.10")
    parser.add_argument("--java", default=shutil.which("java") or "java")
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--probe", type=Path)
    parser.add_argument("--evidence", type=Path)
    run(parser.parse_args())


if __name__ == "__main__":
    main()
