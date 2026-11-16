#!/usr/bin/env python3
"""Assemble one unsigned APK from four byte-compatible APK payloads.

Native libraries vary. Shared engine tables are compared semantically for known
exporter nondeterminism; other assets, code and manifests must match byte-for-byte. Input signatures are discarded; callers must sign output.
"""
import argparse
import copy
import hashlib
import io
import json
import re
from pathlib import Path
import zipfile

MACHINES = {'arm64-v8a': (183, 2), 'armeabi-v7a': (40, 1), 'x86': (3, 1), 'x86_64': (62, 2)}

def signature(name):
    return name.upper().startswith('META-INF/') and name.upper().endswith(('.RSA', '.DSA', '.EC', '.SF', 'MANIFEST.MF'))

def engine_contents(blob):
    """Compare generated data semantically, preserving item/block ordering.

    Independent JVM exporters vary object key order, tag-set iteration order,
    lambda address diagnostics and archive timestamps. No gameplay values,
    registry IDs, code or arbitrary array order may differ.
    """
    with zipfile.ZipFile(io.BytesIO(blob)) as archive:
        names = archive.namelist()
        if len(names) != len(set(names)):
            raise ValueError('Duplicate engine data entries')
        if sum(i.file_size for i in archive.infolist()) > 256 * 1024 * 1024:
            raise ValueError('Engine data exceeds comparison limit')
        result = {n: archive.read(n) for n in names if not archive.getinfo(n).is_dir()}
    manifest_name = 'horizon-resources.json'
    manifest = json.loads(result[manifest_name])
    # Check every retained file hash before ignoring its representation-specific digest.
    for group, entries in manifest.items():
        if group.endswith('_files_sha256'):
            for name, digest in entries.items():
                if name in result:
                    if hashlib.sha256(result[name]).hexdigest() != digest:
                        raise ValueError('Engine data checksum mismatch: ' + name)
                # Public packs intentionally omit original files. Their expected
                # digests remain part of equality instead of being discarded.
    manifest = copy.deepcopy(manifest)
    for group, entries in manifest.items():
        if group.endswith('_files_sha256'):
            for name in entries:
                if name in result:
                    entries[name] = 'verified-packaged-file'
    result[manifest_name] = manifest
    for name in ('render-definitions.json', 'creative-inventory.json', 'block-behaviours.json'):
        if name not in result:
            continue
        data = json.loads(result[name])
        if name == 'creative-inventory.json':
            for tab in data['tabs']:
                # Each inner array is a tag set for one item. Keep outer item order.
                if 'tags' in tab:
                    tab['tags'] = [sorted(tags) for tags in tab['tags']]
        if name == 'block-behaviours.json':
            def normalize(value):
                if isinstance(value, dict):
                    return {k: normalize(v) for k, v in value.items()}
                if isinstance(value, list):
                    return [normalize(v) for v in value]
                if isinstance(value, str) and re.fullmatch(r'<[^<>]+\$\$Lambda/0x[0-9a-fA-F]+>', value):
                    return re.sub(r'/0x[0-9a-fA-F]+>', '/<runtime-address>>', value)
                return value
            data = normalize(data)
        result[name] = data
    return result

def assemble(inputs, output):
    inputs = [Path(p) for p in inputs]
    output = Path(output)
    if len(inputs) != 4 or output.exists():
        raise ValueError('Require four APKs and a new output path')
    shared = None
    native = {}
    engine = None
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
                    if name == 'assets/engine-data.zip':
                        normalized = engine_contents(data)
                        if engine is None:
                            engine = normalized
                        elif normalized != engine:
                            raise ValueError('Generated engine data differs semantically')
                        # Keep the first original, self-consistent pack unchanged.
                        common[name] = shared[name] if shared is not None else data
                    else:
                        common[name] = data
            if len(found) != 1:
                raise ValueError('Each input must have exactly one ABI')
            # Code, manifests and non-engine assets must match byte-for-byte.
            if shared is None:
                shared = common
            elif common != shared:
                raise ValueError('Shared APK payloads differ: ' + ', '.join(sorted(n for n in set(common) | set(shared) if common.get(n) != shared.get(n))))
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
