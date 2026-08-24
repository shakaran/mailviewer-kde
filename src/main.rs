/* main.rs
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

pub mod bridge;

use cxx_qt_lib::{QQmlApplicationEngine, QUrl};

unsafe extern "C" {
    /// Defined in cpp/webengine.cpp
    fn mailviewer_init_web_engine();
    /// Defined in cpp/i18n.cpp
    fn mailviewer_install_translator(directory: *const std::ffi::c_char);
    /// Defined in cpp/app.cpp
    fn mailviewer_app_create(argc: std::ffi::c_int, argv: *mut *mut std::ffi::c_char);
    /// Defined in cpp/app.cpp
    fn mailviewer_app_exec() -> std::ffi::c_int;
}

/// Where the compiled translations are: next to the binary while developing,
/// and under the prefix the binary was installed in, which is /app on flatpak
/// and /usr on a normal install.
fn translations_dir() -> std::ffi::CString {
    let exe = std::env::current_exe().ok();
    for candidate in translation_candidates(exe.as_deref()) {
        if candidate.is_dir() {
            if let Ok(path) = std::ffi::CString::new(candidate.to_string_lossy().as_bytes()) {
                return path;
            }
        }
    }
    std::ffi::CString::new("").unwrap()
}

fn translation_candidates(exe: Option<&std::path::Path>) -> Vec<std::path::PathBuf> {
    let exe_dir = exe.and_then(|exe| exe.parent());
    let mut candidates = Vec::new();

    if let Some(dir) = exe_dir {
        candidates.push(dir.join("i18n"));
        // <prefix>/bin/mailviewer-kde installs its translations here.
        if let Some(prefix) = dir.parent() {
            candidates.push(prefix.join("share/mailviewer-kde/translations"));
        }
    }
    candidates.push(std::path::PathBuf::from("i18n"));
    candidates
}

fn main() {
    // Has to happen before the application exists.
    unsafe { mailviewer_init_web_engine() };

    // QApplication keeps pointing at both for as long as it lives.
    let arguments: Vec<std::ffi::CString> = std::env::args()
        .filter_map(|argument| std::ffi::CString::new(argument).ok())
        .collect();
    let mut argv: Vec<*mut std::ffi::c_char> = arguments
        .iter()
        .map(|argument| argument.as_ptr() as *mut std::ffi::c_char)
        .collect();
    argv.push(std::ptr::null_mut());
    let argc = arguments.len() as std::ffi::c_int;
    let arguments = Box::leak(arguments.into_boxed_slice());
    let argv = Box::leak(argv.into_boxed_slice());
    debug_assert!(!arguments.is_empty());

    unsafe { mailviewer_app_create(argc, argv.as_mut_ptr()) };

    let translations = translations_dir();
    unsafe { mailviewer_install_translator(translations.as_ptr()) };
    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from(
            "qrc:/qt/qml/io/github/alescdb/mailviewer/qml/Main.qml",
        ));
    }

    std::process::exit(unsafe { mailviewer_app_exec() });
}

#[cfg(test)]
mod tests {
    use super::translation_candidates;
    use std::path::{Path, PathBuf};

    fn candidates(exe: &str) -> Vec<PathBuf> {
        translation_candidates(Some(Path::new(exe)))
    }

    #[test]
    fn looks_under_the_prefix_it_was_installed_in() {
        assert!(candidates("/app/bin/mailviewer-kde")
            .contains(&PathBuf::from("/app/share/mailviewer-kde/translations")));
        assert!(candidates("/usr/bin/mailviewer-kde")
            .contains(&PathBuf::from("/usr/share/mailviewer-kde/translations")));
    }

    #[test]
    fn looks_next_to_the_binary_first() {
        assert_eq!(
            candidates("/home/user/mailviewer-kde/target/release/mailviewer-kde")[0],
            PathBuf::from("/home/user/mailviewer-kde/target/release/i18n")
        );
    }
}
