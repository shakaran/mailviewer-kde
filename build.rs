use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(
        QmlModule::new("io.github.alescdb.mailviewer").qml_files(["qml/Main.qml"]),
    )
    .qt_module("Quick")
    .qt_module("WebEngineQuick")
    // The print dialog is a widget, and it prints the pdf QtPdf renders.
    .qt_module("Widgets")
    .qt_module("PrintSupport")
    .qt_module("Pdf")
    .files(["src/bridge.rs"])
    .cpp_file("cpp/webengine.cpp")
    .cpp_file("cpp/i18n.cpp")
    .cpp_file("cpp/app.cpp")
    .cpp_file("cpp/print.cpp")
    .build();
}
