"""Private build entry-point contracts, without running a compiler or downloading inputs."""
import contextlib
import importlib.util
import io
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('android_build', ROOT / 'tools/android_build.py')
BUILD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUILD)


class PrivateAndroidBuild(unittest.TestCase):
    def test_private_mode_requires_an_isolated_generated_only_build(self):
        required = ['--public', '--dev-build', 'legacy17.test', '--dev-version-code', '1',
                    '--app-id', 'dev.taldoshawn.nivora.legacy17.private']
        for omitted in ['--public', '--dev-build', '--dev-version-code', '--app-id']:
            args = required.copy()
            index = args.index(omitted)
            del args[index:index + (1 if omitted == '--public' else 2)]
            with self.subTest(omitted=omitted), patch.object(sys, 'argv', [
                'android_build.py', '--private-build', '--rust-only', *args,
            ]), contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit) as error:
                    BUILD.main()
                self.assertEqual(error.exception.code, 2)
        with patch.object(sys, 'argv', [
            'android_build.py', '--private-build', *required[:-1], 'dev.taldoshawn.nivora',
        ]), contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit) as error:
                BUILD.main()
            self.assertEqual(error.exception.code, 2)

    def test_compiler_receives_private_policy_and_normal_build_resets_it(self):
        with tempfile.TemporaryDirectory(prefix='nivora-private-build-') as directory:
            sdk = Path(directory)
            (sdk / 'build-tools/35.0.0').mkdir(parents=True)
            (sdk / 'ndk/28.2.13676358').mkdir(parents=True)
            for private in [True, False]:
                with self.subTest(private=private), patch.object(sys, 'argv', [
                    'android_build.py', '--rust-only', '--public', '--dev-build', 'legacy17.test',
                    '--dev-version-code', '1', '--app-id', 'dev.taldoshawn.nivora.legacy17.private',
                    '--sdk', str(sdk), '--java-home', str(sdk / 'jdk'),
                    '--ndk-version', '28.2.13676358', '--build-tools-version', '35.0.0',
                    *(['--private-build'] if private else []),
                ]), patch.dict(BUILD.os.environ, {'NIVORA_PRIVATE_TEST': '1'}), \
                        patch.object(BUILD, 'run') as run:
                    BUILD.main()
                    run.assert_called_once()
                    self.assertEqual(run.call_args.args[0][:2], ['cargo', 'build'])
                    env = run.call_args.kwargs['env']
                    self.assertEqual(env['NIVORA_PRIVATE_TEST'], '1' if private else '0')
                    self.assertEqual(env['HORIZON_ANDROID_PUBLIC'], '1')


if __name__ == '__main__':
    unittest.main()
