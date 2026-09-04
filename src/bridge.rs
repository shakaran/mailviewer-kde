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
        // What the message says about itself: empty, "signed" or "encrypted".
        #[qproperty(QString, protection)]
        // What checking the signature said, empty until it is checked: one of
        // good, good-untrusted, no-key, bad, none, error.
        #[qproperty(QString, signature)]
        // Who signed, or why it does not hold. The window turns the pair into
        // a sentence in the language of the user.
        #[qproperty(QString, signature_detail)]
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

        // Sends a pdf written by the view to the printer the user picks, and
        // removes it afterwards. Empty when it went out or was cancelled.
        #[qinvokable]
        fn print_pdf(self: Pin<&mut Message>, path: &QString) -> QString;

        // Checks the signature against the keyring of the application.
        #[qinvokable]
        fn check_signature(self: Pin<&mut Message>);

        // Opens an encrypted message with a key from the keyring. Empty on
        // success, otherwise a word saying what went wrong.
        #[qinvokable]
        fn open_encrypted(self: Pin<&mut Message>, passphrase: &QString) -> QString;
    }
}

use core::pin::Pin;

use cxx_qt::CxxQtType;
use gio::prelude::*;
use mailviewer_core::utils;

unsafe extern "C" {
    /// Defined in cpp/print.cpp
    fn mailviewer_print_pdf(path: *const std::ffi::c_char) -> *const std::ffi::c_char;
}

#[derive(Default)]
pub struct MessageRust {
    from: QString,
    to: QString,
    subject: QString,
    date: QString,
    body: QString,
    error: QString,
    protection: QString,
    signature: QString,
    signature_detail: QString,
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

    fn check_signature(mut self: Pin<&mut Self>) {
        let path = self.path.clone();
        let data = glib::user_data_dir().join("mailviewer-kde");

        let (status, detail) = match crate::crypto::verify(std::path::Path::new(&path), &data) {
            // Mail with more than one signature is rare, and the first one is
            // the one the window has room for.
            Ok(signatures) => match signatures.first() {
                Some(signature) => (signature.status(), signature.detail()),
                None => ("none", String::new()),
            },
            Err(e) => ("error", e),
        };

        self.as_mut().set_signature(QString::from(status));
        self.as_mut().set_signature_detail(QString::from(&detail));
    }

    fn open_encrypted(mut self: Pin<&mut Self>, passphrase: &QString) -> QString {
        let path = self.path.clone();
        let data = glib::user_data_dir().join("mailviewer-kde");

        let inside = match crate::crypto::decrypt(
            std::path::Path::new(&path),
            &data,
            &passphrase.to_string(),
        ) {
            Ok(inside) => inside,
            Err(e) => return QString::from(e.as_str()),
        };

        let allow_remote = *self.allow_remote();
        match render_inside(inside, allow_remote) {
            Ok(message) => {
                self.as_mut().set_body(QString::from(&message.body));
                self.as_mut().set_attachments(names(&message.parts));
                self.as_mut().rust_mut().parts = message.parts;
                // What was inside is now on screen, so the banner changes.
                self.as_mut().set_protection(QString::from("opened"));
                self.as_mut().set_error(QString::from(""));
                QString::from("")
            }
            Err(e) => {
                self.as_mut().set_error(QString::from(&e.to_string()));
                QString::from("failed")
            }
        }
    }

    fn print_pdf(self: Pin<&mut Self>, path: &QString) -> QString {
        let path = gio::File::for_uri(&path.to_string())
            .path()
            .unwrap_or_else(|| std::path::PathBuf::from(path.to_string()));
        let Ok(as_c) = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()) else {
            return QString::from("bad path");
        };

        let error = unsafe { mailviewer_print_pdf(as_c.as_ptr()) };
        let error = if error.is_null() {
            QString::from("")
        } else {
            let error = unsafe { std::ffi::CStr::from_ptr(error) };
            QString::from(error.to_string_lossy().as_ref())
        };

        // Nothing of the message has any business staying on disk.
        let _ = std::fs::remove_file(&path);
        error
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
                self.as_mut()
                    .set_protection(QString::from(message.protection));
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
    protection: &'static str,
    from: String,
    to: String,
    subject: String,
    date: String,
    body: String,
    parts: Vec<mailviewer_core::message::attachment::Attachment>,
}

/// Turns the mime entity that came out of the envelope into something to show,
/// through the same reading and sanitizing every other message goes through.
///
/// It takes a detour through a file because the parser of the core reads
/// files. The file is written in the private folder the attachments already
/// use, and is removed as soon as it has been read.
fn render_inside(
    inside: Vec<u8>,
    allow_remote: bool,
) -> Result<Loaded, Box<dyn std::error::Error>> {
    use std::io::Write;

    let folder = mailviewer_core::message::message::TEMP_FOLDER.clone();
    std::fs::create_dir_all(&folder)?;
    let path = folder.join("opened.eml");

    let mut file = std::fs::File::create(&path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(&inside)?;
    drop(file);

    let loaded = load(&path.to_string_lossy(), allow_remote);
    let _ = std::fs::remove_file(&path);

    let mut loaded = loaded?;
    loaded.protection = "opened";
    Ok(loaded)
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
    use mailviewer_core::message::message::{Message as _, MessageParser, Protection};
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
            protection: match parser.protection() {
                Protection::Encrypted => "encrypted",
                Protection::Signed => "signed",
                Protection::None => "",
            },
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
