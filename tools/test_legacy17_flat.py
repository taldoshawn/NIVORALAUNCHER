#!/usr/bin/env python3
"""Compare restricted native 1.7 flat saves with both official historical servers.

Java is only a disposable development oracle. No modern game inputs are needed.
The worlds, player, logs and verified jars remain under ignored .local/.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import time
import zlib

import anvil_dump
import datafix_worlds
import lab
from test_legacy17_server import checked_server, free_port, isolated_environment

ROOT = Path(__file__).resolve().parent.parent
VERSIONS = {"1.7.2": 4, "1.7.10": 5}
PRESET = "2;7,2x3,2;1;"
NAME = "FlatLightProbe"
EYE = struct.unpack(">f", struct.pack(">f", 1.62))[0]
SOURCES = ["crates/horizon-world/src/legacy17_flat/mod.rs",
           "crates/horizon-world/src/legacy17_flat/chunk.rs",
           "crates/horizon-world/src/legacy17_flat/light.rs",
           "crates/horizon-world/examples/legacy17_flat_fixture.rs",
           "tools/test_legacy17_flat.py"]


def fixture():
    # Independent scene specification: cross-column opaque roof, transparent
    # glass, an upper section and the historical top-layer heightmap edges.
    return ([(x, 8, z, "stone") for z in range(-1, 2) for x in range(14, 19)]
            + [(3, 8, 3, "glass"), (-5, 40, 4, "stone"),
               (6, 255, 5, "stone"), (-8, 31, -8, "stone")])


class FlatBot(datafix_worlds.Bot):
    """A real logged-in 1.7 player keeps the reference chunks ticking."""
    def __init__(self, port, protocol):
        self.failure = None
        super().__init__(port, protocol, NAME)

    def idle(self):
        self.sock.settimeout(1.0)
        while not self.stop.is_set():
            try:
                packet, payload = self.receive()
                if packet == 0:
                    self.send(0, payload)
                elif packet == 1:
                    self.send(0x15, datafix_worlds.mcstring("en_US") + bytes([4, 0, 1, 0, 1]))
                elif packet == 8:
                    x, eye, z, yaw, pitch, ground = struct.unpack(">dddff?", payload)
                    self.send(6, struct.pack(">ddddff?", x, eye - EYE, eye, z, yaw, pitch, ground))
                elif packet == 0x40:
                    self.failure = f"reference disconnected the player: {payload!r}"
                    return
            except socket.timeout:
                continue
            except (EOFError, OSError) as error:
                if not self.stop.is_set():
                    self.failure = str(error)
                return


def column(directory, cx, cz):
    path = directory / "region" / f"r.{cx >> 5}.{cz >> 5}.mca"
    with path.open("rb") as stream:
        stream.seek(((cz & 31) * 32 + (cx & 31)) * 4)
        allocation = int.from_bytes(stream.read(4), "big")
        if not allocation:
            raise AssertionError(f"missing reference column {cx},{cz}: {path}")
        stream.seek((allocation >> 8) * 4096)
        length, codec = struct.unpack(">IB", stream.read(5))
        if codec != 2 or not 1 < length <= (allocation & 255) * 4096 - 4:
            raise AssertionError("unexpected historical region compression/allocation")
        data = stream.read(length - 1)
    level = anvil_dump.read_named(zlib.decompress(data))["Level"]
    assert (level["xPos"], level["zPos"]) == (cx, cz)
    heights = level["HeightMap"][1]
    assert len(heights) == 256
    fields = {"blocks": bytearray(65536), "metadata": bytearray(65536),
              "block_light": bytearray(65536),
              "sky_light": bytearray(15 if y >= heights[c] else 0 for y in range(256) for c in range(256)),
              "heightmap": heights, "biomes": bytes(v & 255 for v in level["Biomes"][1])}
    for section in level["Sections"]:
        start = section["Y"][1] * 4096
        assert 0 <= start < 65536
        assert not any(section.get("Add", ("B", []))[1])
        fields["blocks"][start:start + 4096] = bytes(v & 255 for v in section["Blocks"][1])
        for source, target in [("Data", "metadata"), ("BlockLight", "block_light"), ("SkyLight", "sky_light")]:
            packed = section[source][1]
            assert len(packed) == 2048
            fields[target][start:start + 4096] = bytes((packed[i // 2] >> (4 * (i & 1))) & 15 for i in range(4096))
    return fields


def compare(native, reference, stage):
    differences = []
    for cz in range(-1, 2):
        for cx in range(-1, 2):
            actual, expected = column(native, cx, cz), column(reference, cx, cz)
            for field in actual:
                if actual[field] != expected[field]:
                    i = next(i for i, pair in enumerate(zip(actual[field], expected[field])) if pair[0] != pair[1])
                    where = ((cx * 16 + (i & 15), i >> 8, cz * 16 + ((i >> 4) & 15))
                             if len(actual[field]) == 65536 else (cx, cz, i))
                    differences.append({"field": field, "position": where,
                                        "rust": actual[field][i], "vanilla": expected[field][i],
                                        "count": sum(a != b for a, b in zip(actual[field], expected[field]))})
    result = {"stage": stage, "columns": 9, "block_positions": 589824,
              "light_nibbles": 1179648, "matched": not differences,
              "differences": differences}
    print(json.dumps(result), flush=True)
    return result


def command(server, text):
    lines = server.run(text)
    if datafix_worlds.failed(lines) or any("Cannot " in line for line in lines):
        raise AssertionError(f"reference command failed: {text}: {lines}")


def run_version(args, version, probe):
    jar, sha1 = checked_server(version)
    evidence = (args.evidence / version).resolve()
    evidence.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="work-", dir=evidence))
    homes = work / "homes"
    homes.mkdir()
    env = isolated_environment(homes)
    previous = os.environ.copy()
    os.environ.update(env)
    servers = []
    bot = None
    properties = {"server-ip": "127.0.0.1", "level-type": "FLAT", "generator-settings": PRESET,
                  "level-seed": "0", "gamemode": "1", "difficulty": "0", "max-players": "1",
                  "generate-structures": "false", "spawn-animals": "false", "spawn-monsters": "false",
                  "allow-flight": "true"}

    def start(directory):
        port = free_port()
        server = datafix_worlds.Server(directory, jar, port, java=args.java, props_override=properties)
        servers.append(server)
        return server, port

    def join(server, port):
        player = FlatBot(port, VERSIONS[version])
        server.wait("joined the game", timeout=15)
        command(server, f"tp {NAME} 0.5 4 0.5")
        return player

    def save(server, player):
        # Lighting in these historical builds is partly lazy. Observe an
        # actually joined/ticking world, not only an archive made without users.
        time.sleep(1.1)
        if player.failure:
            raise AssertionError(player.failure)
        # 1.7 writes regions asynchronously; the command's normal completion
        # only queues IO. Flush before independently reading the region files.
        command(server, "save-all flush")

    def native(stage):
        subprocess.run([str(probe), version, stage, str(work / "native")], cwd=ROOT, env=env, check=True)

    try:
        native("generate")
        reference = work / "reference"
        server, _ = start(reference)
        for text in ("setworldspawn 0 4 0", "gamerule doMobSpawning false", "gamerule doDaylightCycle false", "time set 6000"):
            command(server, text)
        server.stop()
        server, port = start(reference)
        bot = join(server, port)
        save(server, bot)
        stages = [compare(work / "native", reference / "world", "generated")]
        native("edit")
        for x, y, z, block in fixture():
            command(server, f"setblock {x} {y} {z} {block} 0")
        save(server, bot)
        stages.append(compare(work / "native", reference / "world", "edited"))
        bot.leave()
        bot = None
        server.stop()

        interop = work / "interop"
        interop.mkdir()
        shutil.copytree(work / "native", interop / "world")
        server, port = start(interop)
        bot = join(server, port)
        for x, y, z, block in fixture():
            command(server, f"testforblock {x} {y} {z} {block} 0")
        save(server, bot)
        stages.append(compare(work / "native", interop / "world", "vanilla_reopened_rust_save"))
        bot.leave()
        bot = None
        server.stop()

        native("remove")
        server, port = start(reference)
        bot = join(server, port)
        for x, y, z, _ in fixture():
            command(server, f"setblock {x} {y} {z} air 0")
        save(server, bot)
        stages.append(compare(work / "native", reference / "world", "removed"))
        result = {"schema": 1, "minecraft": version, "server_sha1": sha1, "preset": PRESET,
                  "stages": stages, "source_sha256": {p: hashlib.sha256((ROOT / p).read_bytes()).hexdigest() for p in SOURCES},
                  "scope": "Native restricted creative-flat terrain/save core only; no gameplay loop, mobs, random ticks, inventories, player movement or Android rendering parity."}
        (evidence / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        failed = [s["stage"] for s in stages if not s["matched"]]
        if failed:
            raise AssertionError(f"{version}: native flat parity failed: {', '.join(failed)}")
        print(f"{version}: all native flat terrain/save stages matched", flush=True)
    finally:
        if bot is not None:
            bot.leave()
        for i, server in enumerate(servers):
            if server.process.poll() is None:
                server.stop()
            (evidence / f"server-{i}.log").write_text("\n".join(server.lines) + "\n", encoding="utf-8")
        os.environ.clear()
        os.environ.update(previous)
        if not args.keep_work:
            shutil.rmtree(work)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", choices=VERSIONS, action="append")
    parser.add_argument("--java", default=shutil.which("java") or "java")
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--probe", type=Path)
    parser.add_argument("--keep-work", action="store_true")
    parser.add_argument("--evidence", type=Path, default=ROOT / ".local" / "legacy17-flat-evidence")
    args = parser.parse_args()
    if not args.skip_build:
        subprocess.run(["cargo", "build", "--locked", "-j", "1", "-p", "horizon-world", "--example", "legacy17_flat_fixture"], cwd=ROOT, check=True)
    probe = (args.probe or lab.cargo_target() / "debug" / "examples" / "legacy17_flat_fixture").resolve()
    if not probe.is_file():
        raise RuntimeError(f"missing native fixture: {probe}")
    failures = []
    for version in args.version or VERSIONS:
        try:
            run_version(args, version, probe)
        except AssertionError as error:
            failures.append(str(error))
            print(str(error), file=sys.stderr, flush=True)
    if failures:
        raise SystemExit("\n".join(failures))


if __name__ == "__main__":
    main()
