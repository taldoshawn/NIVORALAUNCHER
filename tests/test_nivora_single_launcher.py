"""Guard against reintroducing the removed portrait Kotlin launcher.

The public Android entry must take the user directly into the Rust launcher.
A second exported Android Activity risks recreating the legacy eight-button
portrait menu instead of the landscape home with sidebar and game artwork.
"""
from pathlib import Path
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "packaging/android/AndroidManifest.xml"
RUST_HOME = ROOT / "crates/horizon-client/src/android/launcher_pages/reference.rs"
ANDROID = "{http://schemas.android.com/apk/res/android}"


class SingleRustLauncherTests(unittest.TestCase):
    def test_only_the_rust_launcher_is_exposed(self):
        application = ET.parse(MANIFEST).getroot().find("application")
        self.assertIsNotNone(application)
        assert application is not None
        found = []
        for component in list(application):
            if component.tag not in ("activity", "activity-alias"):
                continue
            for intent in component.findall("intent-filter"):
                actions = {item.get(ANDROID + "name") for item in intent.findall("action")}
                categories = {item.get(ANDROID + "name") for item in intent.findall("category")}
                if ("android.intent.action.MAIN" in actions
                    and "android.intent.category.LAUNCHER" in categories):
                    found.append(component)
        self.assertEqual(len(found), 1, "Only one Android launcher icon/entry is allowed")
        self.assertEqual(found[0].tag, "activity-alias")
        self.assertEqual(found[0].get(ANDROID + "targetActivity"),
                         "dev.horizon.android.HorizonActivity")

    def test_normal_candidate_is_official_and_never_auto_published(self):
        workflow = (ROOT / ".github/workflows/android-apk.yml").read_text(encoding="utf-8")
        rust_ui = RUST_HOME.read_text(encoding="utf-8")
        release_workflow = (ROOT / ".github/workflows/android-release.yml").read_text(encoding="utf-8")

        self.assertIn("app_id=dev.taldoshawn.pojav", workflow)
        self.assertIn("'package': 'dev.taldoshawn.pojav'", workflow)
        self.assertNotIn("dev.taldoshawn.nivora.private", workflow)
        self.assertNotIn("inputs.private_test", workflow)
        self.assertNotIn("NIVORA_PRIVATE_TEST", workflow)
        self.assertNotIn("publish_alpha9:", workflow)
        self.assertNotIn("gh release create", workflow)
        self.assertIn('"NIVORA LAUNCHER",', rust_ui)
        self.assertNotIn("TESTE PRIVADO", rust_ui)
        # Publishing remains a deliberate, separate workflow_dispatch action.
        self.assertIn("workflow_dispatch:", release_workflow)
        self.assertIn("universal_candidate:", release_workflow)

    def test_landscape_native_entry_and_no_portait_kotlin_activity(self):
        application = ET.parse(MANIFEST).getroot().find("application")
        assert application is not None
        activity = next(
            entry for entry in application.findall("activity")
            if entry.get(ANDROID + "name") == "dev.horizon.android.HorizonActivity"
        )
        self.assertIn(activity.get(ANDROID + "screenOrientation"),
                      ("sensorLandscape", "landscape", "userLandscape"))
        self.assertEqual(
            activity.find("meta-data").get(ANDROID + "value"),
            "horizon_android",
        )
        for suffix in ("kotlin", "java"):
            for extension in (".kt", ".java"):
                self.assertFalse(
                    (ROOT / "packaging/android" / suffix / "dev/horizon/android"
                     / ("NivoraActivity" + extension)).exists(),
                    "Remove the old standalone eight-button Android menu",
                )
        source = RUST_HOME.read_text(encoding="utf-8")
        self.assertIn("fn reference_shell(", source)
        self.assertIn("fn reference_home(", source)
        self.assertIn('picture(ui, false, hero);', source)


if __name__ == "__main__":
    unittest.main()
