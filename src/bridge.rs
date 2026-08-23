/* bridge.rs
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

//! The message, as a QObject that QML can read.

use cxx_qt_lib::{QString, QStringList};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, from)]
        #[qproperty(QString, to)]
        #[qproperty(QString, subject)]
        #[qproperty(QString, date)]
        #[qproperty(QString, body)]
        #[qproperty(QString, error)]
        #[qproperty(bool, allow_remote)]
        #[qproperty(QStringList, attachments)]
        type Message = super::MessageRust;

        // Reads the file and fills the properties above.
        #[qinvokable]
        fn open(self: Pin<&mut Message>, path: &QString);

        // Renders the same file again, for when allow_remote changes.
        #[qinvokable]
        fn reload(self: Pin<&mut Message>);

        // Writes an attachment where the user asked. Empty on failure.
        #[qinvokable]
        fn save_attachment(self: Pin<&mut Message>, index: i32, path: &QString) -> QString;

        // Writes an attachment to the runtime directory and hands back its
        // path, for opening it with whatever the desktop uses.
        #[qinvokable]
        fn attachment_to_tmp(self: Pin<&mut Message>, index: i32) -> QString;
    }
}

use core::pin::Pin;

use cxx_qt::CxxQtType;
use gio::prelude::*;
use mailviewer_core::utils;

#[derive(Default)]
pub struct MessageRust {
    from: QString,
    to: QString,
    subject: QString,
    date: QString,
    body: QString,
    error: QString,
    allow_remote: bool,
    path: String,
    attachments: QStringList,
    parts: Vec<mailviewer_core::message::attachment::Attachment>,
}

impl qobject::Message {
    fn open(mut self: Pin<&mut Self>, path: &QString) {
        self.as_mut().rust_mut().path = path.to_string();
        self.render();
    }

    fn reload(self: Pin<&mut Self>) {
        self.render();
    }

    fn save_attachment(self: Pin<&mut Self>, index: i32, path: &QString) -> QString {
        let Some(attachment) = self.parts.get(index as usize).cloned() else {
            return QString::from("no such attachment");
        };
        let file = gio::File::for_uri(&path.to_string());
        match utils::spawn_and_wait(
            Some(&glib::MainContext::new()),
            async move { attachment.write_to_file(&file).await },
        ) {
            Ok(()) => QString::from(""),
            Err(e) => QString::from(&e.to_string()),
        }
    }

    fn attachment_to_tmp(self: Pin<&mut Self>, index: i32) -> QString {
        let Some(attachment) = self.parts.get(index as usize).cloned() else {
            return QString::from("");
        };
        match utils::spawn_and_wait(
            Some(&glib::MainContext::new()),
            async move { attachment.write_to_tmp().await },
        ) {
            Ok(file) => QString::from(&file.uri().to_string()),
            Err(e) => {
                log_error(&e.to_string());
                QString::from("")
            }
        }
    }

    fn render(mut self: Pin<&mut Self>) {
        let path = self.path.clone();
        let allow_remote = *self.allow_remote();
        match load(&path, allow_remote) {
            Ok(message) => {
                self.as_mut().set_from(QString::from(&message.from));
                self.as_mut().set_to(QString::from(&message.to));
                self.as_mut().set_subject(QString::from(&message.subject));
                self.as_mut().set_date(QString::from(&message.date));
                self.as_mut().set_body(QString::from(&message.body));
                self.as_mut().set_attachments(names(&message.parts));
                self.as_mut().rust_mut().parts = message.parts;
                self.as_mut().set_error(QString::from(""));
            }
            Err(e) => {
                self.as_mut().set_error(QString::from(&e.to_string()));
            }
        }
    }
}

struct Loaded {
    from: String,
    to: String,
    subject: String,
    date: String,
    body: String,
    parts: Vec<mailviewer_core::message::attachment::Attachment>,
}

/// What the list shows: the name, the type and how big it is.
fn names(parts: &[mailviewer_core::message::attachment::Attachment]) -> QStringList {
    let mut list = QStringList::default();
    for part in parts {
        let mime = part.mime_type.as_deref().unwrap_or("unknown");
        list.append(QString::from(&format!(
            "{}  ({}, {})",
            part.filename,
            mime,
            human_size(part.body.len())
        )));
    }
    list
}

fn human_size(bytes: usize) -> String {
    const UNITS: [&str; 4] = ["B", "kB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[0])
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

fn log_error(message: &str) {
    eprintln!("mailviewer-kde: {message}");
}

/// Everything below is the core doing the work: parsing and sanitizing are the
/// same ones the GTK application uses.
fn load(path: &str, allow_remote: bool) -> Result<Loaded, Box<dyn std::error::Error>> {
    use mailviewer_core::html::Html;
    use mailviewer_core::message::message::{Message as _, MessageParser};
    use mailviewer_core::utils;

    let file = gio::File::for_path(path);
    let loaded = utils::spawn_and_wait(Some(&glib::MainContext::new()), async move {
        let mut parser = MessageParser::new(&file, None).await?;
        parser.parse(None)?;

        let attachments = parser.attachments();
        let body = match parser.body_html() {
            Some(html) => Html::new(&html, false)
                .allow_remote(allow_remote)
                .inline_images(&attachments)
                .safe(),
            None => {
                let text = parser.body_text().unwrap_or_default();
                Html::new(&format!("<pre>{}</pre>", Html::escape(&text)), false)
                    .allow_remote(allow_remote)
                    .safe()
            }
        };

        Ok::<Loaded, Box<dyn std::error::Error>>(Loaded {
            from: parser.from(),
            to: parser.to(),
            subject: parser.subject(),
            date: parser.date(),
            body,
            parts: attachments,
        })
    })?;

    Ok(loaded)
}
