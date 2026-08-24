// SPDX-License-Identifier: GPL-3.0-or-later
//
// QtWebEngine only writes PDFs from QML, so the message goes to a temporary
// file and this prints that file through the system dialog.

#include <QImage>
#include <QPainter>
// With the module prefix: inside a flatpak QtPdf comes from the base app, in
// /app/include, and only the prefixed form is on the include path there.
#include <QtPdf/QPdfDocument>
#include <QPrintDialog>
#include <QPrinter>
#include <QRect>
#include <QSizeF>
#include <QString>

#include <string>

namespace {
// Returned to Rust, which copies it before the next call.
std::string last_error;

const char *fail(const char *message) {
  last_error = message;
  return last_error.c_str();
}
} // namespace

// Returns null when the job was sent or the user cancelled, a message otherwise.
extern "C" const char *mailviewer_print_pdf(const char *path) {
  last_error.clear();

  QPdfDocument document;
  if (document.load(QString::fromUtf8(path)) != QPdfDocument::Error::None) {
    return fail("could not read the document to print");
  }

  QPrinter printer(QPrinter::HighResolution);
  QPrintDialog dialog(&printer);
  if (dialog.exec() != QDialog::Accepted) {
    return nullptr;
  }

  QPainter painter;
  if (!painter.begin(&printer)) {
    return fail("could not start the print job");
  }

  for (int page = 0; page < document.pageCount(); ++page) {
    if (page > 0 && !printer.newPage()) {
      painter.end();
      return fail("the printer stopped taking pages");
    }

    // The page is in points. Rendering at the full resolution of the printer
    // means a 139 megapixel image for an A4 page at 1200 dpi, and 300 dpi is
    // already more than a rendered message needs.
    const int dpi = qMin(printer.resolution(), 300);
    const QSizeF points = document.pagePointSize(page);
    const QSize pixels = (points * dpi / 72.0).toSize();
    const QImage rendered = document.render(page, pixels);
    if (rendered.isNull()) {
      painter.end();
      return fail("could not render a page");
    }

    QRect target(QPoint(0, 0), rendered.size().scaled(painter.viewport().size(),
                                                      Qt::KeepAspectRatio));
    target.moveCenter(painter.viewport().center());
    painter.drawImage(target, rendered);
  }

  painter.end();
  return nullptr;
}
