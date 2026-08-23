/* bridge.rs
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

//! The message, as a QObject that QML can read.

use cxx_qt_lib::QString;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
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
        type Message = super::MessageRust;

        // Reads the file and fills the properties above.
        #[qinvokable]
        fn open(self: Pin<&mut Message>, path: &QString);

        // Renders the same file again, for when allow_remote changes.
        #[qinvokable]
        fn reload(self: Pin<&mut Message>);
    }
}

use core::pin::Pin;

use cxx_qt::CxxQtType;

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
}

impl qobject::Message {
    fn open(mut self: Pin<&mut Self>, path: &QString) {
        self.as_mut().rust_mut().path = path.to_string();
        self.render();
    }

    fn reload(self: Pin<&mut Self>) {
        self.render();
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
        })
    })?;

    Ok(loaded)
}
