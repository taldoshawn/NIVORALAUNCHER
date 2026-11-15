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

