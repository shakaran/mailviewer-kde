// SPDX-License-Identifier: GPL-3.0-or-later
//
// QApplication rather than QGuiApplication: the print dialog is a widget, and
// widgets need one. cxx-qt-lib only binds QGuiApplication, so it is built here.

#include <QApplication>
#include <QCoreApplication>

static int mailviewer_argc = 0;

extern "C" void mailviewer_app_create(int argc, char **argv) {
  mailviewer_argc = argc;
  // QSettings writes where these point, so they come before the application.
  QCoreApplication::setOrganizationName(QStringLiteral("io.github.shakaran"));
  QCoreApplication::setApplicationName(QStringLiteral("mailviewer-kde"));
  // Qt keeps the reference, the caller keeps argc and argv alive.
  new QApplication(mailviewer_argc, argv);
}

extern "C" int mailviewer_app_exec() { return QApplication::exec(); }
