/* keybridge.rs
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

//! The keyring of the application, as a QObject that QML can read.

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
        // One entry per key, as the list shows it.
        #[qproperty(QStringList, keys)]
        // The fingerprint of each, in the same order, for removing one.
        #[qproperty(QStringList, fingerprints)]
        #[qproperty(QString, folder)]
        #[qproperty(QString, error)]
        // Keyring, not Keys: QtQuick already has a Keys attached object, and a
        // type of that name would shadow it.
        type Keyring = super::KeyringRust;

        // Reads the keyring again.
        #[qinvokable]
        fn refresh(self: Pin<&mut Keyring>);

        // Adds what a file holds. Empty on success. Not called import: that
        // is a reserved word in QML.
        #[qinvokable]
        fn add_key(self: Pin<&mut Keyring>, path: &QString) -> QString;

        // Drops a key, private half included. Empty on success.
        #[qinvokable]
        fn remove_key(self: Pin<&mut Keyring>, fingerprint: &QString) -> QString;
    }
}

use core::pin::Pin;

use gio::prelude::*;

use crate::keys::{Key, KeyStore};

#[derive(Default)]
pub struct KeyringRust {
    keys: QStringList,
    fingerprints: QStringList,
    folder: QString,
    error: QString,
}

impl qobject::Keyring {
    fn refresh(mut self: Pin<&mut Self>) {
        let store = match store() {
            Ok(store) => store,
            Err(e) => {
                self.as_mut().set_error(QString::from(&e));
                return;
            }
        };

        self
            .as_mut()
            .set_folder(QString::from(&store.home().display().to_string()));

        match store.list() {
            Ok(keys) => {
                let mut shown = QStringList::default();
                let mut fingerprints = QStringList::default();
                for key in &keys {
                    shown.append(QString::from(&describe(key)));
                    fingerprints.append(QString::from(&key.fingerprint));
                }
                self.as_mut().set_keys(shown);
                self.as_mut().set_fingerprints(fingerprints);
                self.as_mut().set_error(QString::from(""));
            }
            Err(e) => self.as_mut().set_error(QString::from(&e)),
        }
    }

    fn add_key(mut self: Pin<&mut Self>, path: &QString) -> QString {
        let path = local_path(path);
        let result = store().and_then(|store| store.import(&path));
        match result {
            Ok(_) => {
                self.as_mut().refresh();
                QString::from("")
            }
            Err(e) => QString::from(&e),
        }
    }

    fn remove_key(mut self: Pin<&mut Self>, fingerprint: &QString) -> QString {
        let result = store().and_then(|store| store.remove(&fingerprint.to_string()));
        match result {
            Ok(()) => {
                self.as_mut().refresh();
                QString::from("")
            }
            Err(e) => QString::from(&e),
        }
    }
}

/// QML hands over a url, gpg wants a path.
fn local_path(path: &QString) -> std::path::PathBuf {
    let path = path.to_string();
    gio::File::for_uri(&path)
        .path()
        .unwrap_or_else(|| std::path::PathBuf::from(path))
}

fn store() -> Result<KeyStore, String> {
    let data = glib::user_data_dir().join("mailviewer-kde");
    KeyStore::open(&data)
}

/// "MailViewer Test <test@example.com>  (private key, expires 2027-01-01)"
fn describe(key: &Key) -> String {
    let mut notes: Vec<String> = Vec::new();
    if key.secret {
        notes.push("private key".to_string());
    }
    if !key.expires.is_empty() {
        notes.push(match expiry_date(&key.expires) {
            Some(date) => format!("expires {date}"),
            None => "expires".to_string(),
        });
    }

    let who = if key.user.is_empty() {
        key.fingerprint.clone()
    } else {
        key.user.clone()
    };

    if notes.is_empty() {
        who
    } else {
        format!("{}  ({})", who, notes.join(", "))
    }
}

/// gpg gives seconds since the epoch, glib turns them into a date.
fn expiry_date(seconds: &str) -> Option<String> {
    let seconds: i64 = seconds.parse().ok()?;
    let date = glib::DateTime::from_unix_local(seconds).ok()?;
    date.format("%Y-%m-%d").ok().map(|date| date.to_string())
}
