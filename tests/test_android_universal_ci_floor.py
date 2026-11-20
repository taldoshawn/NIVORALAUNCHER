"""Regression: automatic universal builds use the same Android API floor for each ABI.

The APK assembler intentionally requires byte-identical AndroidManifest.xml and
classes.dex. An implicit 27/28 SDK split caused build #60 to fail even though
each per-ABI compilation and audit succeeded.
"""
from pathlib import Path
import subprocess
import unittest


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/android-apk.yml"


class UniversalBuildFloorTests(unittest.TestCase):
    def test_universal_floor_is_enabled_on_branch_push_and_explicit_dispatch(self):
        workflow = WORKFLOW.read_text(encoding="utf-8")
        start = workflow.index("          universal_args=()")
        end = workflow.index("          python3 tools/android_build.py", start)
        block = "\n".join(line[10:] for line in workflow[start:end].splitlines())
        cases = (
            ("push", "refs/heads/codex/private-hud-peripherals", "", True),
            ("workflow_dispatch", "refs/heads/main", "true", True),
            ("workflow_dispatch", "refs/heads/codex/private-hud-peripherals", "false", False),
            ("push", "refs/heads/main", "", False),
        )
        for event, ref, universal, expected in cases:
            with self.subTest(event=event, ref=ref, universal=universal):
                script = (
                    block.replace("${{ inputs.universal }}", universal)
                    .replace("${{ github.event_name }}", event)
                    .replace("${{ github.ref }}", ref)
                )
                self.assertNotIn("${{", script)
                completed = subprocess.run(
                    ["bash", "-e", "-c", script + '\nprintf "%s" "${universal_args[*]}"\n'],
                    text=True, capture_output=True, check=True,
                )
                self.assertEqual(
                    "--universal-floor" in completed.stdout.split(),
                    expected,
                )

    def test_build_script_uses_api_28_floor_when_flag_is_set(self):
        source = (ROOT / "tools/android_build.py").read_text(encoding="utf-8")
        self.assertIn("min_sdk = 27 if legacy and not args.universal_floor else 28", source)


if __name__ == "__main__":
    unittest.main()
