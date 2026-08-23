/* main.rs
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

pub mod bridge;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

unsafe extern "C" {
    /// Defined in cpp/webengine.cpp
    fn mailviewer_init_web_engine();
}

fn main() {
    // Has to happen before the application exists.
    unsafe { mailviewer_init_web_engine() };

    let mut app = QGuiApplication::new();
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
