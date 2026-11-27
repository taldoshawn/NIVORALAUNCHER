"""Generate a temporary historical world, edit a copied region in Rust and reopen on the reference server."""
import argparse
import hashlib
from pathlib import Path
import shutil
import subprocess
import tempfile

from datafix_worlds import ROOT, Server, java, lab, server_jar

VERSIONS = ('1.7.10', '1.8.9', '1.12.2', '1.20.1')


def check(version, work, binary, runtime):
    modern = version == '1.20.1'
    jar, _, _ = server_jar(version)
    runtime = runtime or java(version)
    props = {'server-ip': '127.0.0.1', 'level-type': 'flat', 'generate-structures': 'false',
             'spawn-monsters': 'false', 'spawn-animals': 'false'}
    def start():
        server = Server(work, jar, lab.free_port(), java=runtime, props_override=props)
        if modern:
            server.run('forceload add 0 0')
        return server

    # Legacy servers load the spawn area on startup and may choose a distant spawn.
    server = start()
    try:
        server.run('setworldspawn 0 100 0')
    finally:
        server.stop()
    server = start()
    try:
        for x in (0, 16):
            if modern:
                server.run(f'forceload add {x} 0')
            print(version, server.run(f'setblock {x} 100 0 minecraft:stone'), flush=True)
    finally:
        server.stop()

    source = work / 'world/region/r.0.0.mca'
    digest = hashlib.sha256(source.read_bytes()).digest()
    output = work / 'edited' / source.name
    read = 'get-block' if modern else 'get-numeric-block'
    def get(path, x):
        return subprocess.check_output([binary, read, path, str(x), '100', '0'], text=True).strip()

    stone = 'minecraft:stone' if modern else '1:0'
    assert get(source, 0) == get(source, 16) == stone
    args = ['edit-block-data', source, output, '0', '100', '0', 'minecraft:red_wool'] if modern else [
        'edit-numeric-block', source, output, '0', '100', '0', '35', '14']
    subprocess.run([binary, *args], check=True)
    assert get(output, 0) == ('minecraft:red_wool' if modern else '35:14')
    assert get(output, 16) == stone
    assert hashlib.sha256(source.read_bytes()).digest() == digest
    # Only this disposable world's region is replaced.
    shutil.copyfile(output, source)
    server = start()
    try:
        if modern:
            response = server.run('execute if block 0 100 0 minecraft:red_wool run say nivora-match')
            assert any(line.endswith('[Server] nivora-match') for line in response), response
        else:
            response = server.run('testforblock 0 100 0 minecraft:wool 14')
            assert any('uccess' in line for line in response), response
    finally:
        server.stop()
    print(f'{version}: reference server reopened the Rust edit', flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--version', choices=VERSIONS, action='append')
    parser.add_argument('--java', help='use an installed reference JVM instead of the pinned runtime')
    options = parser.parse_args()
    subprocess.run(['cargo', 'build', '-q', '-p', 'horizon-world', '--bin', 'horizon-world'], cwd=ROOT, check=True)
    for version in options.version or VERSIONS:
        with tempfile.TemporaryDirectory(prefix='nivora-world-') as temp:
            check(version, Path(temp), lab.cargo_target() / 'debug/horizon-world', options.java)
