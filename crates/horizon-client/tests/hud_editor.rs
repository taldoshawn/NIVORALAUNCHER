    #[test]
    fn button_name_text_wins_over_an_open_key_picker() {
        let mut controls = horizon_client::touch::TouchControls::default();
        controls.settings = horizon_client::touch::Settings::default_pojav();
        controls.open_editor();
        let ed = controls.editor.as_mut().unwrap();
        ed.selected_custom = Some(0);
        ed.key_picker = true;
        ed.editing_button_label = true;
        ed.button_label_input.clear();
        let key = controls.settings.custom_buttons[0].key.clone();
        assert!(!controls.capture_custom_text("Novo nome"));
        assert!(horizon_client::touch_ui::apply_editor_button_label_output(&mut controls, &horizon_client::osk::Output::Text("Novo nome".into())));
        assert_eq!(controls.settings.custom_buttons[0].custom_label, "Novo nome");
        assert_eq!(controls.settings.custom_buttons[0].key, key);
        let bytes = serde_json::to_vec(&controls.settings).unwrap();
        let restored: horizon_client::touch::Settings = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored.custom_buttons[0].custom_label, "Novo nome");
    }


use horizon_client::{ime_edit, osk::Output, touch::{CustomButton, Placement, Settings, TouchControls}, touch_ui};
use std::path::{Path, PathBuf};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("pojav-hud-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
}
fn select_name(tc: &mut TouchControls, index: usize) {
    let current = tc.settings.custom_buttons[index].label();
    let ed = tc.editor.as_mut().unwrap();
    ed.selected_custom = Some(index);
    ed.selected = None;
    ed.editing_button_label = true;
    ed.numeric_property = None;
    ed.button_label_input = current;
    ed.save_name_input = "Nome do perfil: não é o botão".into();
    ed.keyboard_request = true;
}
fn edit_ime(tc: &mut TouchControls, old: &str, new: &str, home: &Path) {
    for out in ime_edit::editor_changes(old, new, false) {
        assert!(touch_ui::apply_control_editor_output(tc, &out, home));
    }
}

#[test]
fn launcher_keyboard_edits_existing_created_and_cloned_names_and_reloads_the_active_profile() {
    let scratch = Scratch::new();
    let home = scratch.0.join("profile-2");
    let other = scratch.0.join("profile-1");
    let mut tc = TouchControls::default();
    tc.settings = Settings::default_pojav();
    tc.settings.preset_id = "personalizado".into();
    tc.settings.preset_name = "Meu HUD".into();
    tc.settings.save_to(&other.join("horizon-touch.json")).unwrap();
    let other_before = std::fs::read(other.join("horizon-touch.json")).unwrap();
    let created = tc.settings.custom_buttons.len();
    tc.settings.custom_buttons.push(CustomButton::new("key.keyboard.w".into(), created));
    let cloned = tc.settings.clone_custom_button(0).unwrap();
    tc.open_editor();
    for (index, name) in [(0, "Ataque principal"), (created, "Andar é fácil"), (cloned, "Cópia 🙂")] {
        let untouched = tc.settings.custom_buttons.iter().enumerate()
            .filter(|(i, _)| *i != index).map(|(i, b)| (i, b.clone())).collect::<Vec<_>>();
        select_name(&mut tc, index);
        let initial = tc.settings.custom_buttons[index].label();
        // This is the same keyboard request and output path used by Launcher.
        assert_eq!(touch_ui::take_control_editor_keyboard_text(&mut tc), Some(initial.clone()));
        assert_eq!(touch_ui::take_control_editor_keyboard_text(&mut tc), None);
        edit_ime(&mut tc, &initial, "", &home);
        assert!(tc.settings.custom_buttons[index].custom_label.is_empty());
        edit_ime(&mut tc, "", name, &home);
        assert_eq!(tc.settings.custom_buttons[index].label(), name);
        touch_ui::apply_control_editor_output(&mut tc, &Output::Enter, &home);
        assert!(tc.editor.as_ref().unwrap().status_msg.is_none());
        for (i, before) in untouched { assert_eq!(tc.settings.custom_buttons[i], before); }
        let loaded = Settings::load_from(&home.join("horizon-touch.json"));
        assert_eq!(loaded.custom_buttons[index].custom_label, name);
        let saved = Settings::load_from(&Settings::system_dir(&home, loaded.control_style).join("personalizado.json"));
        assert_eq!(saved.custom_buttons[index].custom_label, name);
    }
    assert_eq!(std::fs::read(other.join("horizon-touch.json")).unwrap(), other_before);
}

#[test]
fn finishing_a_property_commits_buffer_before_hiding_keyboard() {
    let scratch = Scratch::new();
    let mut tc = TouchControls::default();
    tc.settings = Settings::default_pojav();
    tc.open_editor();
    select_name(&mut tc, 0);
    let initial = touch_ui::take_control_editor_keyboard_text(&mut tc).unwrap();
    edit_ime(&mut tc, &initial, "Novo", &scratch.0);
    tc.editor.as_mut().unwrap().keyboard_finish_requested = true;
    edit_ime(&mut tc, "Novo", "Novo nome", &scratch.0);
    assert!(touch_ui::finish_control_editor_keyboard(&mut tc, &scratch.0));
    assert!(!touch_ui::finish_control_editor_keyboard(&mut tc, &scratch.0));
    assert!(!tc.editor.as_ref().unwrap().editing_button_label);
    assert_eq!(Settings::load_from(&scratch.0.join("horizon-touch.json")).custom_buttons[0].custom_label, "Novo nome");
}

#[test]
fn one_bottom_right_handle_resizes_each_axis_while_preserving_the_opposite_corner() {
    use horizon_client::touch::editor::{resize_handle, resize_placement};
    for aspect in [0.6, 1.0, 16.0 / 9.0, 20.0 / 9.0] {
        let original = Placement::new(0.4, 0.4, 0.12);
        let before = original.rect(aspect);
        for delta in [[0.02, 0.0], [0.0, 0.03], [0.02, 0.03], [-0.01, -0.01]] {
            let next = resize_placement(original, delta, aspect).rect(aspect);
            for i in 0..2 { assert!((next[i] - before[i]).abs() < 0.00001); }
            assert!((next[2] - before[2] - delta[0]).abs() < 0.00001);
            assert!((next[3] - before[3] - delta[1]).abs() < 0.00001);
        }
        let viewport = [480.0 * aspect, 480.0];
        let [x,y,w,h] = resize_handle([20.0, 30.0, 80.0, 50.0], viewport);
        assert_eq!([x,y,w,h], [98.0,78.0,24.0,24.0]);
        let [x,y,w,h] = resize_handle([viewport[0]-10.0, 470.0, 10.0, 10.0], viewport);
        assert!(x >= 0.0 && y >= 0.0 && x+w <= viewport[0] && y+h <= viewport[1]);
    }
}

#[test]
fn numeric_keyboard_uses_its_own_buffer_and_preserves_button_name_and_other_axis() {
    let scratch = Scratch::new();
    let mut tc = TouchControls::default();
    tc.settings = Settings::default_pojav();
    tc.open_editor();
    let before = tc.settings.custom_buttons[0].clone();
    let ed = tc.editor.as_mut().unwrap();
    ed.selected_custom = Some(0);
    ed.editing_button_label = true;
    ed.numeric_property = Some(0);
    ed.property_viewport = [854.0, 480.0];
    ed.button_label_input = "80".into();
    ed.save_name_input = "Não usar este nome".into();
    ed.keyboard_request = true;
    assert_eq!(touch_ui::take_control_editor_keyboard_text(&mut tc), Some("80".into()));
    edit_ime(&mut tc, "80", "100", &scratch.0);
    assert_eq!(tc.settings.custom_buttons[0], before); // Numeric entry commits on confirmation.
    assert!(touch_ui::apply_control_editor_output(&mut tc, &Output::Enter, &scratch.0));
    let after = &tc.settings.custom_buttons[0];
    assert_eq!(after.custom_label, before.custom_label);
    for mode in 0..3 {
        assert_eq!(after.layouts[mode].height_ratio, before.layouts[mode].height_ratio);
        assert_eq!(after.layouts[mode].size, before.layouts[mode].size);
        assert!((after.layouts[mode].rect(854.0/480.0)[2] * 854.0 - 100.0).abs() < 0.01);
    }
}
