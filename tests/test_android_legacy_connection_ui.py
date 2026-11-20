"""Regression checks for Android multiplayer modal touch priority.

These source-level checks complement the Android cross-compilation in CI.
They prevent a future HUD redesign from masking ConnectScreen Cancel or the
disconnect dialog while keeping custom control presets intact.
"""
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]
GAME = ROOT / "crates/horizon-client/src/game.rs"
TOUCH = ROOT / "crates/horizon-client/src/game/android_input.rs"
NETWORK = ROOT / "crates/horizon-client/src/net/legacy18.rs"
REMOTE = ROOT / "crates/horizon-client/src/game/remote.rs"


class ConnectionModalTests(unittest.TestCase):
    def test_connect_cancel_before_overlays(self):
        source = TOUCH.read_text(encoding="utf-8")
        start = source.index("pub(super) fn android_aux_touch(")
        cancel = source.index("if t.phase == TouchPhase::Started && self.remote_loading()", start)
        controls = source.index("floating_menu_touch(", start)
        self.assertLess(cancel, controls)
        self.assertIn("self.connect_screen_click()?", source[cancel:controls])
        self.assertIn("self.pad.touch.ignore(t.id)", source[cancel:controls])
        self.assertIn("pub(super) fn connect_cancel(", REMOTE.read_text(encoding="utf-8"))

    def test_disconnected_back_button_has_direct_tap(self):
        source = TOUCH.read_text(encoding="utf-8")
        start = source.index("if t.phase == TouchPhase::Started && self.android_multiplayer_disconnected()")
        next_overlay = source.index("floating_menu_touch(", start)
        direct = source[start:next_overlay]
        self.assertIn("button_y = h.floor() - 52.0", direct)
        self.assertIn("button_x = (w.ceil() / 2.0).floor() - 100.0", direct)
        self.assertIn("WindowEvent::MouseInput", direct)
        self.assertIn("ElementState::Pressed, ElementState::Released", direct)

    def test_blocking_screens_hide_and_do_not_capture_hud(self):
        source = GAME.read_text(encoding="utf-8")
        self.assertIn("let connection_menu_overlay = self.remote_loading()", source)
        self.assertIn("if !connection_menu_overlay", source)
        self.assertIn("if self.pad.touch.editor.is_none() && !connection_menu_overlay", source)
        self.assertIn("let menu_custom_hit = !blocking_connection_menu", source)
        self.assertIn("let inventory_control = !blocking_connection_menu", source)

    def test_android_system_back_cancels_active_connect_screen(self):
        source = GAME.read_text(encoding="utf-8")
        block = source[source.index("// Android's system Back gesture/key"):
                       source.index("follow_keymap(&mut event);", source.index("// Android's system Back gesture/key"))]
        self.assertIn('target_os = "android"', block)
        self.assertIn("self.remote_loading()", block)
        self.assertIn("self.connect_cancel(self.gui_size()).is_some()", block)
        self.assertIn("NamedKey::BrowserBack", block)
        self.assertIn("self.leave_server(None)", block)

    def test_legacy_network_identifies_closure_phase_without_offline_fallback(self):
        source = NETWORK.read_text(encoding="utf-8")
        self.assertIn('legacy_transport_error(e, lang, "during login")', source)
        self.assertIn('legacy_transport_error(e, lang, "after login")', source)
        self.assertIn("connection closed {phase} without a server reason", source)
        self.assertIn("This server requires a signed-in Minecraft account", source)


if __name__ == "__main__":
    unittest.main()
