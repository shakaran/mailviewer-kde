/* main.rs
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

pub mod bridge;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

unsafe extern "C" {
    /// Defined in cpp/webengine.cpp
    fn mailviewer_init_web_engine();
    /// Defined in cpp/i18n.cpp
    fn mailviewer_install_translator(directory: *const std::ffi::c_char);
}

/// Where the compiled translations are: next to the binary while developing,
/// and the usual share directory once installed.
fn translations_dir() -> std::ffi::CString {
    let candidates = [
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join("i18n"))),
        Some(std::path::PathBuf::from("i18n")),
        Some(std::path::PathBuf::from(
            "/usr/share/mailviewer-kde/translations",
        )),
    ];
    for candidate in candidates.into_iter().flatten() {
        if candidate.is_dir() {
            if let Ok(path) = std::ffi::CString::new(candidate.to_string_lossy().as_bytes()) {
                return path;
            }
        }
    }
    std::ffi::CString::new("").unwrap()
}

fn main() {
    // Has to happen before the application exists.
    unsafe { mailviewer_init_web_engine() };

    let mut app = QGuiApplication::new();

    let translations = translations_dir();
    unsafe { mailviewer_install_translator(translations.as_ptr()) };
    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from(
            "qrc:/qt/qml/io/github/alescdb/mailviewer/qml/Main.qml",
        ));
    }

    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
