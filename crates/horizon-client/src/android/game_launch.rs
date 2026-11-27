//! Frontend-independent game launch parameters. No launcher widgets or config reads.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum GameVersion {
    #[default]
    #[serde(rename = "modern")]
    Modern,
    #[serde(rename = "1.8.9")]
    Legacy18,
    #[serde(rename = "1.7.10")]
    Legacy17Protocol5,
    #[serde(rename = "1.7.2")]
    Legacy17Protocol4,
}

impl GameVersion {
    pub(crate) const ALL: [Self; 4] = [
        Self::Modern,
        Self::Legacy18,
        Self::Legacy17Protocol5,
        Self::Legacy17Protocol4,
    ];

    pub(crate) fn from_previous_config(config: &serde_json::Value) -> Self {
        if config["legacy17"].as_bool() == Some(true) {
            Self::Legacy17Protocol5
        } else if config["legacy18"].as_bool() == Some(true) {
            Self::Legacy18
        } else {
            Self::Modern
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Modern => "26.3",
            Self::Legacy18 => "1.8.9",
            Self::Legacy17Protocol5 => "1.7.10",
            Self::Legacy17Protocol4 => "1.7.2",
        }
    }

    pub(crate) fn is_legacy(self) -> bool {
        self != Self::Modern
    }

    fn protocol_arg(self) -> Option<&'static str> {
        match self {
            Self::Modern => None,
            Self::Legacy18 => Some("--protocol-47"),
            Self::Legacy17Protocol5 => Some("--protocol-5"),
            Self::Legacy17Protocol4 => Some("--protocol-4"),
        }
    }
}

#[derive(Clone)]
pub(crate) struct LaunchRequest {
    pub resources: PathBuf,
    pub game_home: PathBuf,
    pub session_file: Option<PathBuf>,
    pub offline_name: String,
    pub server: String,
    pub control_socket: Option<PathBuf>,
    pub extra_arguments: Vec<String>,
    pub version: GameVersion,
    pub open_gl: bool,
    pub gpu_batching: bool,
}
impl LaunchRequest {
    pub fn arguments(&self) -> Vec<String> {
        let mut args = vec![
            "--resources".into(),
            self.resources.to_string_lossy().into_owned(),
            "--game-dir".into(),
            self.game_home.to_string_lossy().into_owned(),
        ];
        if let Some(socket) = &self.control_socket {
            args.extend([
                "--control-socket".into(),
                socket.to_string_lossy().into_owned(),
            ]);
        }
        if let Some(session) = &self.session_file {
            args.extend([
                "--session-file".into(),
                session.to_string_lossy().into_owned(),
            ]);
        } else if !self.offline_name.is_empty() {
            args.extend(["--name".into(), self.offline_name.clone()]);
        }
        if !self.server.is_empty() {
            args.extend(["--server".into(), self.server.clone()]);
        }
        args.extend(self.extra_arguments.iter().cloned());
        args.extend(self.version.protocol_arg().map(str::to_owned));
        args
    }
}
/// The native host owns the window lifecycle; a future frontend supplies this request.
#[cfg(target_os = "android")]
pub(crate) fn start(
    request: LaunchRequest,
) -> horizon_client::Result<Box<dyn winit::application::ApplicationHandler>> {
    if !request.game_home.is_absolute() || !request.resources.is_absolute() {
        return Err("Game and resource directories must be absolute".into());
    }
    let previous = std::env::current_dir()?;
    std::env::set_current_dir(&request.game_home)?;
    if request.open_gl {
        horizon_client::android_runtime::configure_graphics(true);
    } else {
        horizon_client::android_runtime::configure_graphics(false);
        horizon_client::android_runtime::set_prefer_vulkan(true);
    }
    horizon_client::android_runtime::configure_gpu(request.gpu_batching);
    crate::game::android_reset_bundled_mods();
    let result = crate::game::android_game(request.arguments());
    if result.is_err() {
        let _ = std::env::set_current_dir(previous);
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> LaunchRequest {
        LaunchRequest {
            resources: "resources with spaces".into(),
            game_home: "profile".into(),
            session_file: None,
            offline_name: "Offline".into(),
            server: "example.test".into(),
            control_socket: None,
            extra_arguments: vec![],
            version: GameVersion::Modern,
            open_gl: false,
            gpu_batching: false,
        }
    }
    #[test]
    fn historical_versions_launch_with_their_own_protocol() {
        for (version, flag) in [
            (GameVersion::Legacy17Protocol4, "--protocol-4"),
            (GameVersion::Legacy17Protocol5, "--protocol-5"),
            (GameVersion::Legacy18, "--protocol-47"),
        ] {
            let mut request = request();
            request.version = version;
            assert_eq!(request.arguments().last().map(String::as_str), Some(flag));
            let saved = serde_json::to_string(&version).unwrap();
            assert_eq!(
                serde_json::from_str::<GameVersion>(&saved).unwrap(),
                version
            );
        }
        assert!(
            !request()
                .arguments()
                .iter()
                .any(|arg| arg.starts_with("--protocol-"))
        );
    }

    #[test]
    fn old_boolean_preferences_restore_the_previous_protocol() {
        use serde_json::json;
        for (saved, expected) in [
            (json!({}), GameVersion::Modern),
            (json!({"legacy18": true}), GameVersion::Legacy18),
            (
                json!({"legacy17": true, "legacy18": true}),
                GameVersion::Legacy17Protocol5,
            ),
            (json!({"legacy17": true}), GameVersion::Legacy17Protocol5),
        ] {
            assert_eq!(GameVersion::from_previous_config(&saved), expected);
        }
    }

    #[test]
    fn session_identity_wins_over_offline_name() {
        let mut r = request();
        r.session_file = Some("private/session.json".into());
        let args = r.arguments();
        assert!(!args.iter().any(|a| a == "--name"));
        assert!(
            args.windows(2)
                .any(|a| a == ["--session-file", "private/session.json"])
        );
    }
    #[test]
    fn paths_are_arguments_not_shell_text_and_snapshot_is_independent() {
        let mut r = request();
        let snapshot = r.arguments();
        r.offline_name = "Changed".into();
        assert_eq!(snapshot[1], "resources with spaces");
        assert!(snapshot.windows(2).any(|a| a == ["--name", "Offline"]));
        assert!(snapshot.windows(2).any(|a| a == ["--game-dir", "profile"]));
        assert!(
            snapshot
                .windows(2)
                .any(|a| a == ["--server", "example.test"])
        );
    }
}
