// SPDX-License-Identifier: GPL-3.0-or-later
//
// QtWebEngineQuick::initialize() has to run before the GUI application is
// created, and cxx-qt-lib does not bind QtWebEngine, so this is the one piece
// of C++ in the project.

#include <QtWebEngineQuick/QtWebEngineQuick>

extern "C" void mailviewer_init_web_engine() {
  QtWebEngineQuick::initialize();
}
