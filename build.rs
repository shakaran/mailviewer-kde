use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(
        QmlModule::new("io.github.alescdb.mailviewer").qml_files(["qml/Main.qml"]),
    )
    .qt_module("Quick")
    .qt_module("WebEngineQuick")
    .files(["src/bridge.rs"])
    .cpp_file("cpp/webengine.cpp")
    .build();
}
