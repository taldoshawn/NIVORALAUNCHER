#!/usr/bin/env python3
"""Package tracked documentation and original examples for Android releases."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import zipfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--source-commit', required=True)
    args = parser.parse_args()
    if not re.fullmatch(r'[0-9a-f]{40}', args.source_commit):
        parser.error('--source-commit must be a complete Git commit')
    docs_commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
    roots = ['README.md', 'LICENSE', 'THIRD_PARTY_NOTICES.md', 'docs',
             'examples/pojav-cursor', 'crates/horizon-mod-sdk/examples/coords-hud',
             'crates/horizon-mod-sdk/examples/ruby-content']
    files = subprocess.check_output(['git', 'ls-files', '-z', '--', *roots]).decode().split('\0')
    args.output.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(args.output / 'Pojav-Rust-Documentacao-e-Exemplos.zip', 'w', zipfile.ZIP_DEFLATED) as archive:
        for name in sorted(filter(None, files)):
            archive.write(name, name)
        archive.writestr('LEIA-PRIMEIRO.txt', 'Guia: docs/pojav-development.md\nPastas: docs/android-content-folders.md\nSource: https://github.com/taldoshawn/pojav\nOs exemplos requerem o SDK no checkout completo. Consulte as limitacoes documentadas.\n')
        archive.writestr('SOURCE.json', json.dumps({'code_commit': args.source_commit,
            'documentation_commit': docs_commit, 'repository': 'taldoshawn/pojav'}, indent=2))
    root = Path('examples/pojav-cursor')
    with zipfile.ZipFile(args.output / 'Pojav-Rust-Cursor-Exemplo.zip', 'w', zipfile.ZIP_DEFLATED) as archive:
        for name in sorted(filter(None, files)):
            path = Path(name)
            if path.is_relative_to(root):
                archive.write(path, path.relative_to(root))
    print('Packaged documentation and original cursor example.')


if __name__ == '__main__':
    main()
