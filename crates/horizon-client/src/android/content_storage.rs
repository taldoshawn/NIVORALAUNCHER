//! App-owned external inboxes. Imports never overwrite an installed file.
use std::{
    fs,
    io::{self, Read, Write},
    path::Path,
};
const FOLDERS: &[(&str, &str)] = &[
    ("resourcepacks", "resourcepacks"),
    ("mods", "horizon-mods"),
    ("shaderpacks", "shaderpacks"),
    ("controls", "controls"),
];
#[derive(Default, Debug)]
pub struct Report {
    pub imported: usize,
    pub existing: usize,
}

pub fn initialize(game: &Path, external: Option<&Path>) -> io::Result<()> {
    for (public, installed) in FOLDERS {
        fs::create_dir_all(game.join(installed))?;
        if let Some(external) = external {
            fs::create_dir_all(external.join(public))?;
        }
    }
    Ok(())
}
pub fn import(game: &Path, external: &Path) -> io::Result<Report> {
    initialize(game, Some(external))?;
    let mut report = Report::default();
    let mut visited = 0;
    for (public, installed) in FOLDERS {
        copy_missing(
            &external.join(public),
            &game.join(installed),
            0,
            &mut visited,
            &mut report,
        )?;
    }
    Ok(report)
}
// Android's untrusted_app SELinux policy forbids hard links, including within
// private app data. renameat2(RENAME_NOREPLACE) provides atomic publication
// without linking or overwriting a destination created by another writer.
#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    any(
        target_arch = "aarch64",
        target_arch = "arm",
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "riscv64"
    )
))]
fn publish_missing(source: &Path, destination: &Path) -> io::Result<()> {
    use std::{
        ffi::{CString, c_long},
        os::unix::ffi::OsStrExt,
    };
    unsafe extern "C" {
        fn syscall(number: c_long, ...) -> c_long;
    }
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    const RENAME_AT_2: c_long = 276;
    #[cfg(target_arch = "arm")]
    const RENAME_AT_2: c_long = 382;
    #[cfg(target_arch = "x86")]
    const RENAME_AT_2: c_long = 353;
    #[cfg(target_arch = "x86_64")]
    const RENAME_AT_2: c_long = 316;
    let from = CString::new(source.as_os_str().as_bytes())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let to = CString::new(destination.as_os_str().as_bytes())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    // Both paths stay alive through the syscall. AT_FDCWD = -100, RENAME_NOREPLACE = 1.
    let result = unsafe {
        syscall(
            RENAME_AT_2,
            -100 as c_long,
            from.as_ptr(),
            -100 as c_long,
            to.as_ptr(),
            1 as c_long,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
#[cfg(not(all(
    any(target_os = "linux", target_os = "android"),
    any(
        target_arch = "aarch64",
        target_arch = "arm",
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "riscv64"
    )
)))]
fn publish_missing(source: &Path, destination: &Path) -> io::Result<()> {
    fs::hard_link(source, destination)
}

fn copy_missing(
    source: &Path,
    destination: &Path,
    depth: usize,
    visited: &mut usize,
    report: &mut Report,
) -> io::Result<()> {
    if depth > 32 {
        return Err(io::Error::other("Pasta de conteúdo excede 32 níveis"));
    }
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        *visited += 1;
        if *visited > 100_000 {
            return Err(io::Error::other("Pasta de conteúdo excede 100000 arquivos"));
        }
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let from = entry.path();
        let to = destination.join(name);
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            // Never follow an installed link out of the game content directory.
            if let Ok(meta) = fs::symlink_metadata(&to) {
                if !meta.is_dir() || meta.file_type().is_symlink() {
                    report.existing += 1;
                    continue;
                }
            }
            fs::create_dir_all(&to)?;
            copy_missing(&from, &to, depth + 1, visited, report)?;
        } else if kind.is_file() {
            if fs::symlink_metadata(&to).is_ok() {
                report.existing += 1;
                continue;
            }
            let temp =
                destination.join(format!(".pojav-import-{}-{}", std::process::id(), visited));
            let mut owned_temp = false;
            let result = (|| {
                let mut input = fs::File::open(&from)?.take(1024 * 1024 * 1024 + 1);
                let mut output = fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&temp)?;
                owned_temp = true;
                let bytes = io::copy(&mut input, &mut output)?;
                if bytes > 1024 * 1024 * 1024 {
                    return Err(io::Error::other("Arquivo de conteúdo excede 1 GiB"));
                }
                output.flush()?;
                output.sync_all()?;
                // Publish only when no installed file occupies the destination.
                match publish_missing(&temp, &to) {
                    Ok(()) => report.imported += 1,
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => report.existing += 1,
                    Err(e) => return Err(e),
                }
                Ok(())
            })();
            if owned_temp {
                let _ = fs::remove_file(&temp);
            }
            result?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atomic_publication_preserves_a_destination_created_after_the_scan() {
        let root = std::env::temp_dir().join(format!("pojav-content-race-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let result = std::panic::catch_unwind(|| {
            let staged = root.join("staged");
            let installed = root.join("installed");
            fs::write(&staged, b"complete incoming pack").unwrap();
            fs::write(&installed, b"existing player pack").unwrap();
            assert_eq!(
                publish_missing(&staged, &installed).unwrap_err().kind(),
                io::ErrorKind::AlreadyExists
            );
            assert_eq!(fs::read(&installed).unwrap(), b"existing player pack");
            assert_eq!(fs::read(&staged).unwrap(), b"complete incoming pack");
            let new_path = root.join("new");
            publish_missing(&staged, &new_path).unwrap();
            assert_eq!(fs::read(new_path).unwrap(), b"complete incoming pack");
        });
        fs::remove_dir_all(root).unwrap();
        result.unwrap();
    }
    #[test]
    fn imports_nested_packs_without_overwriting_player_content_or_following_links() {
        let root = std::env::temp_dir().join(format!("pojav-content-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let result = std::panic::catch_unwind(|| {
            let game = root.join("game");
            let external = root.join("external");
            initialize(&game, Some(&external)).unwrap();
            fs::write(external.join("mods/example.hmod"), b"new mod").unwrap();
            fs::create_dir_all(external.join("resourcepacks/Pack/assets")).unwrap();
            fs::write(
                external.join("resourcepacks/Pack/assets/texture.png"),
                b"texture",
            )
            .unwrap();
            fs::write(
                game.join("horizon-mods/example.hmod"),
                b"installed player mod",
            )
            .unwrap();
            #[cfg(unix)]
            std::os::unix::fs::symlink(&game, external.join("mods/escape")).unwrap();
            let report = import(&game, &external).unwrap();
            assert_eq!(report.imported, 1);
            assert_eq!(report.existing, 1);
            assert_eq!(
                fs::read(game.join("horizon-mods/example.hmod")).unwrap(),
                b"installed player mod"
            );
            assert_eq!(
                fs::read(game.join("resourcepacks/Pack/assets/texture.png")).unwrap(),
                b"texture"
            );
            assert!(!game.join("horizon-mods/escape").exists());
            assert_eq!(import(&game, &external).unwrap().imported, 0);
        });
        fs::remove_dir_all(root).unwrap();
        result.unwrap();
    }
}
