//! Black-box checks of the untrusted-directory rules (M1.7, #96): a symlinked
//! runtime dir or lock file is refused, a looser dir is tightened to 0700, and
//! a launch after a refusal still works once the problem is gone.
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::PathBuf;

use dictation_platform::single_instance::{Config, Launch, Message, acquire};

fn message() -> Message {
    Message {
        cwd: "/".into(),
        args: vec!["app".into()],
    }
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("si-untrusted-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn mode(path: &std::path::Path) -> u32 {
    std::fs::symlink_metadata(path)
        .unwrap()
        .permissions()
        .mode()
        & 0o777
}

#[test]
fn a_symlinked_runtime_dir_is_refused_and_its_target_left_alone() {
    let root = scratch("dirlink");
    let target = root.join("target");
    std::fs::create_dir(&target).unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
    let link = root.join("link");
    symlink(&target, &link).unwrap();
    let error = acquire(&Config::in_dir(&link, "a"), &message()).expect_err("refused");
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    assert_eq!(mode(&target), 0o755, "target must not be chmodded");
    assert_eq!(
        std::fs::read_dir(&target).unwrap().count(),
        0,
        "nothing created in target"
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_symlinked_lock_file_is_refused_and_its_target_untouched() {
    let root = scratch("locklink");
    let dir = root.join("run");
    let config = Config::in_dir(&dir, "a");
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let victim = root.join("victim");
    std::fs::write(&victim, b"keep").unwrap();
    symlink(&victim, config.lock_path()).unwrap();
    let error = acquire(&config, &message()).expect_err("refused");
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    assert_eq!(std::fs::read(&victim).unwrap(), b"keep");
    assert!(!config.socket_path().exists());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn loose_dirs_are_tightened_and_a_fresh_dir_is_0700() {
    for loose in [0o755, 0o777, 0o750, 0o701] {
        let root = scratch("loose");
        let dir = root.join("run");
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(loose)).unwrap();
        let Launch::Primary(_i) = acquire(&Config::in_dir(&dir, "a"), &message()).unwrap() else {
            panic!("primary expected");
        };
        assert_eq!(mode(&dir), 0o700, "from {loose:o}");
        std::fs::remove_dir_all(&root).unwrap();
    }
    let root = scratch("fresh");
    let dir = root.join("a/b/run");
    let Launch::Primary(_i) = acquire(&Config::in_dir(&dir, "a"), &message()).unwrap() else {
        panic!("primary expected");
    };
    assert_eq!(mode(&dir), 0o700);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_regular_file_in_place_of_the_dir_is_refused() {
    let root = scratch("file");
    let path = root.join("run");
    std::fs::write(&path, b"x").unwrap();
    assert!(acquire(&Config::in_dir(&path, "a"), &message()).is_err());
    std::fs::remove_dir_all(&root).unwrap();
}
