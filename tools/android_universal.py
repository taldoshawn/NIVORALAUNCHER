#!/usr/bin/env python3
"""Assemble one unsigned APK from four byte-compatible APK payloads.

Only native libraries vary. Conflicting shared assets, code, manifests or ABI
headers abort assembly. Input signatures are discarded; callers must sign output.
"""
import argparse
from pathlib import Path
import zipfile

MACHINES = {'arm64-v8a': (183, 2), 'armeabi-v7a': (40, 1), 'x86': (3, 1), 'x86_64': (62, 2)}

def signature(name):
    return name.upper().startswith('META-INF/') and name.upper().endswith(('.RSA', '.DSA', '.EC', '.SF', 'MANIFEST.MF'))

def assemble(inputs, output):
    inputs = [Path(p) for p in inputs]
    output = Path(output)
    if len(inputs) != 4 or output.exists():
        raise ValueError('Require four APKs and a new output path')
    shared = None
    native = {}
    for path in inputs:
        with zipfile.ZipFile(path) as apk:
            names = apk.namelist()
            if len(names) != len(set(names)):
                raise ValueError('Duplicate APK entries')
            common = {}
            found = set()
            for name in names:
                entry = apk.getinfo(name)
                if entry.is_dir() or signature(name):
                    continue
                if entry.file_size > 256 * 1024 * 1024:
                    raise ValueError('Oversized input member')
                data = apk.read(name)
                if name.startswith('lib/'):
                    parts = name.split('/')
                    if len(parts) != 3 or parts[1] not in MACHINES or not parts[2].endswith('.so'):
                        raise ValueError('Unexpected native member')
                    abi = parts[1]
                    machine, elf_class = MACHINES[abi]
                    if len(data)<20 or data[:4]!=b'\x7fELF' or data[4]!=elf_class or data[5]!=1 or int.from_bytes(data[18:20], 'little')!=machine:
                        raise ValueError('Native architecture mismatch')
                    if name in native:
                        raise ValueError('Duplicate native architecture')
                    native[name] = data
                    found.add(abi)
                else:
                    common[name] = data
            if len(found) != 1:
                raise ValueError('Each input must have exactly one ABI')
            # Native build tooling stamps a per-ABI version in BuildConfig, so
            # shared code must match exactly; never silently pick one variant.
            if shared is None:
                shared = common
            elif common != shared:
                raise ValueError('Shared APK payloads differ; universal assembly is unsafe')
    if {name.split('/')[1] for name in native} != set(MACHINES):
        raise ValueError('Missing native architecture')
    result = {**shared, **native}
    if not {'AndroidManifest.xml', 'classes.dex', 'resources.arsc'}.issubset(result):
        raise ValueError('Missing Android application payload')
    temp = output.with_suffix('.partial')
    try:
        with zipfile.ZipFile(temp, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=6) as apk:
            for name, data in sorted(result.items()):
                info = zipfile.ZipInfo(name, (2009, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                info.external_attr = 0o100644 << 16
                apk.writestr(info, data)
        temp.replace(output)
    finally:
        temp.unlink(missing_ok=True)
    return output

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', action='append', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    assemble(args.input, args.output)
    print(f'Universal unsigned APK: {args.output.stat().st_size / 1024**2:.1f} MiB')
