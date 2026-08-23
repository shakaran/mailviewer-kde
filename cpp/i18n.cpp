// SPDX-License-Identifier: GPL-3.0-or-later
//
// QTranslator is not bound by cxx-qt-lib either, and it has to be installed on
// the application before the QML engine loads anything.

#include <QCoreApplication>
#include <QLibraryInfo>
#include <QLocale>
#include <QString>
#include <QTranslator>

extern "C" void mailviewer_install_translator(const char *directory) {
  auto *translator = new QTranslator(QCoreApplication::instance());
  if (translator->load(QLocale(), QStringLiteral("mailviewer-kde"),
                       QStringLiteral("_"), QString::fromUtf8(directory))) {
    QCoreApplication::installTranslator(translator);
  } else {
    delete translator;
  }
}
