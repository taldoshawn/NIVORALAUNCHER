//! In-app GitHub release update detection and installer for Android.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::{
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

pub const CURRENT_VERSION: &str = "0.1.0";
pub const REPO: &str = "taldoshawn/NIVORALAUNCHER";

fn device_abi() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64-v8a",
        "arm" => "armeabi-v7a",
        "x86" => "x86",
        "x86_64" => "x86_64",
        _ => "unsupported",
    }
}

fn matching_apk<'a>(assets: &'a [Value], abi: &str) -> Option<&'a Value> {
    if !matches!(abi, "arm64-v8a" | "armeabi-v7a" | "x86" | "x86_64") {
        return None;
    }
    let suffix = format!("-{abi}.apk");
    assets.iter().find(|asset| {
        asset["name"]
            .as_str()
            .is_some_and(|name| name.ends_with(&suffix))
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReleaseInfo {
    pub version: String,
    pub tag_name: String,
    pub title: String,
    pub changelog: String,
    pub download_url: Option<String>,
    pub asset_name: String,
    pub size_bytes: u64,
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum UpdateStatus {
    Idle,
    Checking,
    Available(ReleaseInfo),
    Downloading {
        progress: f32,
        downloaded: u64,
        total: u64,
    },
    Downloaded {
        path: PathBuf,
        version: String,
    },
    Error(String),
    Dismissed,
}

static STATUS: Mutex<Option<UpdateStatus>> = Mutex::new(None);
static CHECKING: AtomicBool = AtomicBool::new(false);
static DOWNLOADING: AtomicBool = AtomicBool::new(false);

pub fn get_status() -> UpdateStatus {
    STATUS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or(UpdateStatus::Idle)
}

pub fn set_status(new_status: UpdateStatus) {
    *STATUS.lock().unwrap_or_else(|e| e.into_inner()) = Some(new_status);
}

pub fn dismiss() {
    set_status(UpdateStatus::Dismissed);
}

const MAX_APK_BYTES: u64 = 1024 * 1024 * 1024;

fn trusted_asset(info: &ReleaseInfo) -> Result<(), String> {
    let name = &info.asset_name;
    if name.is_empty()
        || name.len() > 180
        || !name.ends_with(".apk")
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
        || name.contains("..")
    {
        return Err("Nome inválido do APK na release oficial.".into());
    }
    let url = info.download_url.as_deref().unwrap_or("");
    let prefix = format!("https://github.com/{REPO}/releases/download/");
    if !url.starts_with(&prefix)
        || url.contains(['?', '#', '\\'])
        || url.split('/').any(|part| part == ".." || part == ".")
        || url.contains("%")
        || !url.ends_with(&format!("/{name}"))
    {
        return Err("O download não pertence às releases oficiais do NIVORA.".into());
    }
    if info.size_bytes == 0 || info.size_bytes > MAX_APK_BYTES {
        return Err("Tamanho do APK fora do limite permitido.".into());
    }
    if !info
        .sha256
        .as_deref()
        .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err("A release precisa informar um SHA-256 válido antes da instalação.".into());
    }
    Ok(())
}

/// Bounded streaming: an interrupted or altered download never becomes an APK.
fn copy_verified(
    reader: &mut impl Read,
    file: &mut impl Write,
    size: u64,
    expected: &str,
    mut progress: impl FnMut(u64),
) -> Result<(), String> {
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut downloaded = 0u64;
    loop {
        let n = reader
            .read(&mut buffer)
            .map_err(|e| format!("Falha no download: {e}"))?;
        if n == 0 {
            break;
        }
        downloaded += n as u64;
        if downloaded > size || downloaded > MAX_APK_BYTES {
            return Err("O download excedeu o tamanho informado pela release.".into());
        }
        file.write_all(&buffer[..n])
            .map_err(|e| format!("Falha ao gravar o APK: {e}"))?;
        hash.update(&buffer[..n]);
        progress(downloaded);
    }
    if downloaded != size {
        return Err("Download incompleto. Tente novamente.".into());
    }
    let digest = format!("{:x}", hash.finalize());
    if !digest.eq_ignore_ascii_case(expected) {
        return Err("Falha de integridade SHA-256. O APK não será instalado.".into());
    }
    Ok(())
}

/// Parses version string into numeric tuple (major, minor, patch, prerelease).
pub fn parse_version(raw: &str) -> (u32, u32, u32, u32) {
    let clean = raw.trim().trim_start_matches('v').trim_start_matches('V');
    let mut parts = clean.split(&['.', '-', '+'][..]);
    let major = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let build = parts.find_map(|s| s.parse().ok()).unwrap_or(0);
    (major, minor, patch, build)
}

/// Returns true if `remote` is strictly higher than `current`.
pub fn is_newer_version(remote: &str, current: &str) -> bool {
    parse_version(remote) > parse_version(current)
}

/// Fetches the latest release from the repository and parses release metadata.
pub fn check_for_update() -> Result<Option<ReleaseInfo>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let response = ureq::get(&url)
        .set("User-Agent", &format!("Horizon-Android/{CURRENT_VERSION}"))
        .set("Accept", "application/vnd.github.v3+json")
        .timeout(std::time::Duration::from_secs(12))
        .call();
    let resp = match response {
        Ok(response) => response,
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(error) => return Err(format!("Could not reach GitHub Releases: {error}")),
    };

    let val: Value = resp
        .into_json()
        .map_err(|e| format!("Invalid release JSON response: {e}"))?;

    let tag = val["tag_name"].as_str().unwrap_or("").trim().to_owned();
    let name = val["name"].as_str().unwrap_or(&tag).trim().to_owned();
    let changelog = val["body"]
        .as_str()
        .unwrap_or("New release with improvements and fixes.")
        .trim()
        .to_owned();

    let remote_ver = tag.trim_start_matches('v').trim_start_matches('V');
    if !is_newer_version(remote_ver, CURRENT_VERSION) {
        return Ok(None);
    }

    let mut download_url = None;
    let mut asset_name = format!("horizon-{tag}.apk");
    let mut size_bytes = 0u64;
    let mut sha256 = None;

    if let Some(assets) = val["assets"].as_array() {
        if let Some(asset) = matching_apk(assets, device_abi()) {
            asset_name = asset["name"].as_str().unwrap_or_default().to_owned();
            download_url = asset["browser_download_url"].as_str().map(str::to_owned);
            size_bytes = asset["size"].as_u64().unwrap_or(0);
            sha256 = asset["digest"]
                .as_str()
                .and_then(|s| s.strip_prefix("sha256:"))
                .map(str::to_owned);
        }
    }

    if download_url.is_none() {
        return Ok(None);
    }
    let info = ReleaseInfo {
        version: remote_ver.to_owned(),
        tag_name: tag,
        title: name,
        changelog,
        download_url,
        asset_name,
        size_bytes,
        sha256,
    };
    trusted_asset(&info)?;
    Ok(Some(info))
}

/// Initiates an update check on a background worker thread.
pub fn check_in_background() {
    if CHECKING.swap(true, Ordering::SeqCst) {
        return;
    }
    set_status(UpdateStatus::Checking);
    let worker = std::thread::Builder::new()
        .name("UpdateChecker".into())
        .spawn(|| {
            match check_for_update() {
                Ok(Some(info)) => {
                    log::info!("New update available: {}", info.version);
                    set_status(UpdateStatus::Available(info));
                }
                Ok(None) => {
                    log::info!("Horizon is up to date (version {CURRENT_VERSION})");
                    set_status(UpdateStatus::Idle);
                }
                Err(err) => {
                    log::warn!("Update check failed: {err}");
                    set_status(UpdateStatus::Error(err));
                }
            }
            CHECKING.store(false, Ordering::SeqCst);
        });
    if let Err(error) = worker {
        CHECKING.store(false, Ordering::SeqCst);
        set_status(UpdateStatus::Error(format!(
            "Não foi possível iniciar a verificação: {error}"
        )));
    }
}

/// Downloads the APK and triggers automatic install.
pub fn download_and_install_in_background(info: ReleaseInfo, home: PathBuf) {
    if DOWNLOADING.swap(true, Ordering::SeqCst) {
        return;
    }
    let Some(url) = info.download_url.clone() else {
        set_status(UpdateStatus::Error("No APK asset found in release.".into()));
        DOWNLOADING.store(false, Ordering::SeqCst);
        return;
    };

    set_status(UpdateStatus::Downloading {
        progress: 0.0,
        downloaded: 0,
        total: info.size_bytes,
    });

    let worker = std::thread::Builder::new()
        .name("UpdateDownloader".into())
        .spawn(move || {
            let res = (|| -> Result<PathBuf, String> {
                trusted_asset(&info)?;
                let updates_dir = home.join("updates");
                std::fs::create_dir_all(&updates_dir).map_err(|e| e.to_string())?;
                let apk_path = updates_dir.join(&info.asset_name);

                let resp = ureq::get(&url)
                    .set("User-Agent", &format!("Horizon-Android/{CURRENT_VERSION}"))
                    .timeout(std::time::Duration::from_secs(60))
                    .call()
                    .map_err(|e| format!("Failed to download update: {e}"))?;

                let partial = apk_path.with_extension("apk.partial");
                let download = (|| -> Result<(), String> {
                    let mut reader = resp.into_reader();
                    let mut file = std::fs::File::create(&partial).map_err(|e| e.to_string())?;
                    let mut last_report = std::time::Instant::now();
                    copy_verified(
                        &mut reader,
                        &mut file,
                        info.size_bytes,
                        info.sha256.as_deref().ok_or("SHA-256 ausente")?,
                        |downloaded| {
                            if last_report.elapsed() >= std::time::Duration::from_millis(100) {
                                set_status(UpdateStatus::Downloading {
                                    progress: downloaded as f32 / info.size_bytes as f32,
                                    downloaded,
                                    total: info.size_bytes,
                                });
                                last_report = std::time::Instant::now();
                            }
                        },
                    )?;
                    file.sync_all().map_err(|e| e.to_string())?;
                    drop(file);
                    std::fs::rename(&partial, &apk_path).map_err(|e| e.to_string())
                })();
                if download.is_err() {
                    let _ = std::fs::remove_file(&partial);
                }
                download?;
                Ok(apk_path)
            })();

            match res {
                Ok(apk_path) => {
                    log::info!("Update downloaded to: {}", apk_path.display());
                    set_status(UpdateStatus::Downloaded {
                        path: apk_path.clone(),
                        version: info.version.clone(),
                    });
                    trigger_install(&apk_path);
                }
                Err(err) => {
                    log::warn!("Download error: {err}");
                    set_status(UpdateStatus::Error(err));
                }
            }
            DOWNLOADING.store(false, Ordering::SeqCst);
        });
    if let Err(error) = worker {
        DOWNLOADING.store(false, Ordering::SeqCst);
        set_status(UpdateStatus::Error(format!(
            "Não foi possível iniciar o download: {error}"
        )));
    }
}

/// Invokes Android package installer through JNI.
pub fn trigger_install(apk_path: &Path) {
    #[cfg(target_os = "android")]
    {
        crate::platform::with_context(|env, activity| {
            let path_str = apk_path.to_string_lossy().into_owned();
            let j_path = env.new_string(&path_str)?;
            let cls = env.get_object_class(activity)?;
            let _ = env.call_static_method(
                &cls,
                "installApk",
                "(Landroid/content/Context;Ljava/lang/String;)V",
                &[
                    jni::objects::JValue::Object(activity),
                    jni::objects::JValue::Object(&j_path.into()),
                ],
            );
            Ok(())
        });
    }
    #[cfg(not(target_os = "android"))]
    {
        log::info!("Triggering APK install on path: {}", apk_path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_selects_the_devices_architecture_not_the_first_apk() {
        let assets = serde_json::json!([
            {"name": "Pojav-Rust-1-arm64-v8a.apk"},
            {"name": "Pojav-Rust-1-x86_64.apk"},
            {"name": "Pojav-Rust-1-armeabi-v7a.apk"},
            {"name": "Pojav-Rust-1-x86.apk"}
        ]);
        let assets = assets.as_array().unwrap();
        for abi in ["arm64-v8a", "armeabi-v7a", "x86", "x86_64"] {
            assert_eq!(
                matching_apk(assets, abi).unwrap()["name"],
                format!("Pojav-Rust-1-{abi}.apk")
            );
        }
        assert!(matching_apk(&assets[..1], "x86").is_none());
        assert!(matching_apk(assets, "unsupported").is_none());
        assert!(matching_apk(&[], "arm64-v8a").is_none());
    }

    #[test]
    fn version_parsing_and_comparison() {
        assert_eq!(parse_version("1.0.0"), (1, 0, 0, 0));
        assert_eq!(parse_version("v1.0.1"), (1, 0, 1, 0));
        assert_eq!(parse_version("1.2.3-dev.4"), (1, 2, 3, 4));

        assert!(is_newer_version("1.0.1", "1.0.0"));
        assert!(is_newer_version("v1.1.0", "1.0.0"));
        assert!(is_newer_version("2.0.0", "1.9.9"));
        assert!(!is_newer_version("1.0.0", "1.0.0"));
        assert!(!is_newer_version("0.9.9", "1.0.0"));
    }
}

#[cfg(test)]
mod integrity_tests {
    use super::*;
    use std::io::Cursor;
    fn release() -> ReleaseInfo {
        ReleaseInfo {
            version: "0.1.0".into(),
            tag_name: "v0.1.0".into(),
            title: "NIVORA".into(),
            changelog: String::new(),
            download_url: Some(format!(
                "https://github.com/{REPO}/releases/download/v0.1.0/Nivora-arm64-v8a.apk"
            )),
            asset_name: "Nivora-arm64-v8a.apk".into(),
            size_bytes: 3,
            sha256: Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into()),
        }
    }
    #[test]
    fn only_official_bounded_assets_with_safe_names_and_digests_are_accepted() {
        let original = release();
        assert!(trusted_asset(&original).is_ok());
        for name in [
            "../app.apk",
            "/app.apk",
            "a\\app.apk",
            "..apk",
            "a.apk/evil",
            "",
        ] {
            assert!(
                trusted_asset(&ReleaseInfo {
                    asset_name: name.into(),
                    ..original.clone()
                })
                .is_err()
            );
        }
        for url in [
            "https://evil.test/app.apk",
            "http://github.com/taldoshawn/NIVORALAUNCHER/releases/download/v1/Nivora-arm64-v8a.apk",
            "https://github.com/taldoshawn/NIVORALAUNCHERSITE/releases/download/v1/Nivora-arm64-v8a.apk",
        ] {
            assert!(
                trusted_asset(&ReleaseInfo {
                    download_url: Some(url.into()),
                    ..original.clone()
                })
                .is_err()
            );
        }
        for size in [0, MAX_APK_BYTES + 1] {
            assert!(
                trusted_asset(&ReleaseInfo {
                    size_bytes: size,
                    ..original.clone()
                })
                .is_err()
            );
        }
        for digest in [None, Some("bad".into()), Some("x".repeat(64))] {
            assert!(
                trusted_asset(&ReleaseInfo {
                    sha256: digest,
                    ..original.clone()
                })
                .is_err()
            );
        }
    }
    #[test]
    fn streaming_verifies_known_digest_and_rejects_tampering_truncation_and_excess() {
        let expected = release().sha256.unwrap();
        let mut output = Vec::new();
        copy_verified(&mut Cursor::new(b"abc"), &mut output, 3, &expected, |_| {}).unwrap();
        assert_eq!(output, b"abc");
        for input in [b"ab".as_slice(), b"abd", b"abcd"] {
            assert!(
                copy_verified(
                    &mut Cursor::new(input),
                    &mut Vec::new(),
                    3,
                    &expected,
                    |_| {}
                )
                .is_err()
            );
        }
    }
    #[test]
    fn interrupted_network_and_full_storage_abort_verification() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::new(
                    std::io::ErrorKind::ConnectionReset,
                    "interrupted",
                ))
            }
        }
        let expected = release().sha256.unwrap();
        assert!(copy_verified(&mut Broken, &mut Vec::new(), 3, &expected, |_| {}).is_err());
        let mut full = &mut [0u8; 1][..];
        assert!(copy_verified(&mut Cursor::new(b"abc"), &mut full, 3, &expected, |_| {}).is_err());
    }
}
