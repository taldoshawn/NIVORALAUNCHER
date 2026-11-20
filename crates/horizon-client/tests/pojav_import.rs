use horizon_client as horizon_client_render;
use horizon_client_render::touch::{Settings, TouchControls};
use std::path::PathBuf;
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "nivora-private-hud-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn mush_layout() -> Settings {
    horizon_client_render::touch::pojav::import(
        &serde_json::from_str::<serde_json::Value>(include_str!("fixtures/mush-v8.json")).unwrap(),
    )
    .unwrap()
}
#[test]
fn mush_v8_preserves_44_controls_specials_glfw_and_thin_swipe_regions_after_reload() {
    let scratch = Scratch::new();
    let mut settings = mush_layout();
    assert_eq!(settings.custom_buttons.len(), 44);
    assert_eq!(settings.custom_buttons[4].key, "key.keyboard.t");
    assert_eq!(settings.custom_buttons[5].key, "key.keyboard.f"); // source binds F, despite its F5 label
    assert_eq!(settings.custom_buttons[6].key, "key.keyboard.tab");
    assert_eq!(settings.custom_buttons[7].key, "key.keyboard.space");
    assert_eq!(settings.custom_buttons[8].key, "key.keyboard.left.shift");
    assert_eq!(settings.custom_buttons[32].key, "launcher.menu");
    assert_eq!(
        settings.custom_buttons[37].active_tokens(),
        vec!["key.keyboard.w", "key.keyboard.d"]
    );
    settings.sanitize();
    assert!(!settings.custom_buttons[37].label().is_empty());
    assert!(settings.custom_buttons[12].layouts[0].swipeable);
    assert_eq!(settings.custom_buttons[12].layouts[0].roundness, 52.);
    assert_eq!(settings.custom_buttons[32].layouts[0].opacity, 0.); // intentionally invisible in source
    assert!(settings.custom_buttons[0].layouts[0].visible_in_menu);
    assert!(
        (settings.custom_buttons[12].layouts[0].rect(854. / 480.)[3] * 480. - 4.952381).abs()
            < 0.001
    );
    assert!(
        (settings.custom_buttons[12].layouts[0].rect(854. / 480.)[2] * 854. - 173.33333).abs()
            < 0.001
    );
    settings
        .save_to(&scratch.0.join("horizon-touch.json"))
        .unwrap();
    let reloaded = Settings::load_from(&scratch.0.join("horizon-touch.json"));
    assert_eq!(reloaded.custom_buttons.len(), 44);
    assert_eq!(reloaded.pojav_info, settings.pojav_info);
    let exported = horizon_client_render::touch::pojav::export(&reloaded).unwrap();
    let again = horizon_client_render::touch::pojav::import(&exported).unwrap();
    for (a, b) in settings.custom_buttons.iter().zip(&again.custom_buttons) {
        assert_eq!(a.custom_label, b.custom_label);
        assert_eq!(a.active_tokens(), b.active_tokens());
        assert_eq!(
            a.pojav_geometry.as_ref().unwrap().source["dynamicX"],
            b.pojav_geometry.as_ref().unwrap().source["dynamicX"]
        );
        assert_eq!(a.layouts[0].button_color, b.layouts[0].button_color);
    }
}
#[test]
fn mush_diagonal_touch_releases_both_keys_and_dynamic_layout_survives_aspect_changes() {
    for (w, h) in [(854, 480), (1600, 720), (1920, 1080)] {
        let mut tc = TouchControls::default();
        tc.settings = mush_layout();
        tc.viewport(w, h);
        let p = tc.custom_placement(37);
        let r = p.rect(tc.aspect());
        let point = [r[0] + r[2] / 2., r[1] + r[3] / 2.];
        tc.update(777, point, true, false);
        let down = tc.key_events();
        assert!(
            down.contains(&("key.keyboard.w".into(), true)),
            "{w} {down:?}"
        );
        assert!(down.contains(&("key.keyboard.d".into(), true)));
        tc.update(777, point, false, true);
        let up = tc.key_events();
        assert!(up.contains(&("key.keyboard.w".into(), false)));
        assert!(up.contains(&("key.keyboard.d".into(), false)));
        for i in 0..44 {
            assert!(
                tc.custom_placement(i)
                    .rect(tc.aspect())
                    .iter()
                    .all(|v| v.is_finite())
            );
        }
    }
}

#[test]
fn drawers_toggle_their_children_and_roundtrip_with_one_joystick() {
    use serde_json::json;
    let button = json!({"name":"D","width":50.,"height":50.,"dynamicX":"100","dynamicY":"100","keycodes":[0,0,0,0]});
    let mut child = button.clone();
    child["name"] = json!("W");
    child["keycodes"] = json!([87, 0, 0, 0]);
    let mut joystick = button.clone();
    joystick["dynamicX"] = json!("0");
    joystick["dynamicY"] = json!("300");
    joystick["absolute"] = json!(true);
    let s=horizon_client_render::touch::pojav::import(&json!({"version":8,"scaledAt":100.,"mControlDataList":[],"mDrawerDataList":[{"properties":button,"orientation":"RIGHT","buttonProperties":[child]}],"mJoystickDataList":[joystick]})).unwrap();
    let mut tc = TouchControls::default();
    tc.settings = s;
    tc.viewport(854, 480);
    assert!(!tc.custom_visible_for_state(1, false));
    let r = tc.custom_placement(0).rect(tc.aspect());
    let point = [r[0] + r[2] / 2., r[1] + r[3] / 2.];
    tc.update(1, point, true, false);
    tc.update(1, point, false, true);
    assert!(tc.custom_visible_for_state(1, false));
    let out = horizon_client_render::touch::pojav::export(&tc.settings).unwrap();
    assert_eq!(out["mDrawerDataList"].as_array().unwrap().len(), 1);
    assert_eq!(out["mJoystickDataList"].as_array().unwrap().len(), 1);
    let again = horizon_client_render::touch::pojav::import(&out).unwrap();
    assert_eq!(again.custom_buttons.len(), 2);
    assert!(again.pojav_joystick.is_some());
}
#[test]
fn toggle_controls_keeps_its_own_button_accessible_and_preserves_preferences() {
    let mut tc = TouchControls::default();
    tc.settings = mush_layout();
    tc.viewport(854, 480);
    tc.controls_hidden = true;
    assert_eq!(tc.settings.custom_buttons[1].key, "controls.toggle");
    assert!(tc.custom_visible_for_state(1, false));
    assert!(!tc.custom_visible_for_state(2, false));
    assert_eq!(tc.settings.custom_buttons.len(), 44);
}

#[test]
fn editing_and_cloning_imported_geometry_preserves_other_buttons() {
    use horizon_client_render::touch::{pojav, property_input};
    let mut tc = TouchControls::default();
    tc.settings = mush_layout();
    tc.viewport(1600, 720);
    let index = 37;
    let aspect = tc.aspect();
    let before = tc.custom_placement(index).rect(aspect);
    let untouched = tc.settings.custom_buttons[12].clone();
    let copied = tc
        .settings
        .clone_custom_button_for_viewport(index, aspect)
        .unwrap();
    assert_eq!(tc.settings.custom_buttons[12], untouched);
    assert!(tc.settings.custom_buttons[index].pojav_geometry.is_some());
    assert!(tc.settings.custom_buttons[copied].pojav_geometry.is_none());
    let copy = tc.custom_placement(copied).rect(aspect);
    assert!((copy[2] - before[2]).abs() < 0.00001);
    assert!((copy[3] - before[3]).abs() < 0.00001);
    pojav::materialize(&mut tc.settings, index, aspect);
    let after = tc.custom_placement(index).rect(aspect);
    for (a, b) in before.into_iter().zip(after) {
        assert!((a - b).abs() < 0.00001);
    }
    let p = &mut tc.settings.custom_buttons[index].layouts[0];
    assert!(property_input::apply_number(p, 0, "180", [1600., 720.]));
    let changed = p.rect(aspect);
    assert!((changed[2] * 1600. - 180.).abs() < 0.001);
    assert!((changed[3] - after[3]).abs() < 0.00001);
    assert_eq!(tc.settings.custom_buttons[12], untouched);
}

#[test]
fn document_import_activates_mush_and_reopens_the_persisted_preset() {
    let home = Scratch::new();
    let input = include_bytes!("fixtures/mush-v8.json");
    let meta = Settings::detect_and_import_preset(&home.0, input, "mush (1).json").unwrap();
    assert!(!meta.is_default);
    assert!(meta.name.contains("mush"), "{}", meta.name);
    let active = Settings::load_from(&home.0.join("horizon-touch.json"));
    assert_eq!(active.custom_buttons.len(), 44);
    assert_eq!(active.preset_id, meta.id);
    let file =
        Settings::system_dir(&home.0, active.control_style).join(format!("{}.json", meta.id));
    let persisted = Settings::load_from(&file);
    assert_eq!(persisted.custom_buttons, active.custom_buttons);
    let export = horizon_client_render::touch::pojav::export(&active).unwrap();
    Settings::detect_and_import_preset(
        &home.0,
        &serde_json::to_vec(&export).unwrap(),
        "roundtrip.json",
    )
    .unwrap();
    assert_eq!(
        Settings::load_from(&home.0.join("horizon-touch.json"))
            .custom_buttons
            .len(),
        44
    );
}

#[test]
fn bounded_expressions_accept_math_but_reject_code_unknown_keys_and_invalid_results() {
    use serde_json::json;
    let base = json!({"version":8,"scaledAt":100.,"mControlDataList":[{"name":"","width":50.,"height":50.,"dynamicX":"px(20) + (${width} * 2 - 10) / 2","dynamicY":"1 + 2 * 3","keycodes":[87,0,0,0]}]});
    let settings = horizon_client_render::touch::pojav::import(&base).unwrap();
    let rect = settings.custom_buttons[0].layouts[0].rect(854. / 480.);
    assert!((rect[0] * 854. - 65.).abs() < 0.001);
    assert!((rect[1] * 480. - 7.).abs() < 0.001);
    assert!(!settings.custom_buttons[0].label().is_empty());
    for bad in [
        "1 / 0".to_owned(),
        "${unknown}".to_owned(),
        "java.lang.Runtime.exec(1)".to_owned(),
        format!("{}1{}", "(".repeat(100), ")".repeat(100)),
        "1".repeat(9000),
    ] {
        let mut input = base.clone();
        input["mControlDataList"][0]["dynamicX"] = json!(bad);
        assert!(horizon_client_render::touch::pojav::import(&input).is_err());
    }
    let mut input = base;
    input["mControlDataList"][0]["keycodes"] = json!([-999, 0, 0, 0]);
    assert!(horizon_client_render::touch::pojav::import(&input).is_err());
}
