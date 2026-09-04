/* crypto.rs
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

//! Checking a signature against the keyring of the application.
//!
//! gmime does the work, which matters: getting the bytes that were signed back
//! byte for byte is the hard part of PGP/MIME, and doing it by hand is how
//! verification quietly starts saying "bad signature" for well formed mail.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use gmime::prelude::Cast;
use gmime::traits::{
    CertificateExt, MessageExt, MultipartSignedExt, ParserExt, SignatureExt, SignatureListExt,
    StreamExt,
};
use gmime::{glib::translate::IntoGlib, Parser, StreamMem};

use crate::keys::KeyStore;

/// gpg reads GNUPGHOME from the environment, which belongs to the whole
/// process, so one check at a time.
static GNUPG: Mutex<()> = Mutex::new(());

/// What gmime reports for one signature, in the words the window uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Signature {
    /// The signature holds and the key is in the keyring.
    Good { who: String },
    /// The signature holds, but nothing says the key belongs to who it claims.
    GoodUnknownKey { who: String },
    /// The key is not in the keyring, so there is nothing to check against.
    NoKey { who: String },
    /// The message does not match the signature, or the key cannot be used.
    Bad { why: Reason },
}

/// Why a signature does not hold. A word rather than a sentence, so the
/// window can say it in the language of the user.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Changed,
    KeyRevoked,
    KeyExpired,
    SignatureExpired,
    GpgError,
}

impl Reason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Reason::Changed => "changed",
            Reason::KeyRevoked => "key-revoked",
            Reason::KeyExpired => "key-expired",
            Reason::SignatureExpired => "signature-expired",
            Reason::GpgError => "gpg-error",
        }
    }
}

impl Signature {
    /// A word for the window, which turns it into a sentence.
    pub fn status(&self) -> &'static str {
        match self {
            Signature::Good { .. } => "good",
            Signature::GoodUnknownKey { .. } => "good-untrusted",
            Signature::NoKey { .. } => "no-key",
            Signature::Bad { .. } => "bad",
        }
    }

    /// Who signed, or why it does not hold.
    pub fn detail(&self) -> String {
        match self {
            Signature::Good { who }
            | Signature::GoodUnknownKey { who }
            | Signature::NoKey { who } => who.clone(),
            Signature::Bad { why } => why.as_str().to_string(),
        }
    }

    fn from_status(status: i32, who: String) -> Self {
        // A bitfield, not a list, and an empty one is not an error: gmime sets
        // GREEN when gpg both accepts the signature and trusts the key, and
        // leaves everything at zero for a signature that holds made with a key
        // nobody has certified, which is every key a user imports by hand.
        const GREEN: i32 = 2;
        const RED: i32 = 4;
        const KEY_REVOKED: i32 = 16;
        const KEY_EXPIRED: i32 = 32;
        const SIG_EXPIRED: i32 = 64;
        const KEY_MISSING: i32 = 128;
        const SYS_ERROR: i32 = 2048;

        if status & KEY_MISSING != 0 {
            return Signature::NoKey { who };
        }
        if status & RED != 0 {
            return Signature::Bad {
                why: Reason::Changed,
            };
        }
        if status & KEY_REVOKED != 0 {
            return Signature::Bad {
                why: Reason::KeyRevoked,
            };
        }
        if status & KEY_EXPIRED != 0 {
            return Signature::Bad {
                why: Reason::KeyExpired,
            };
        }
        if status & SIG_EXPIRED != 0 {
            return Signature::Bad {
                why: Reason::SignatureExpired,
            };
        }
        if status & SYS_ERROR != 0 {
            return Signature::Bad {
                why: Reason::GpgError,
            };
        }
        if status & GREEN != 0 {
            Signature::Good { who }
        } else {
            Signature::GoodUnknownKey { who }
        }
    }
}

/// Checks every signature the message carries, against the keyring in
/// `data_dir`. An empty list means the message carries none.
pub fn verify(path: &Path, data_dir: &Path) -> Result<Vec<Signature>, String> {
    let store = KeyStore::open(data_dir)?;
    let _guard = point_gpg_at(store.home());

    let data = std::fs::read(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    let stream = StreamMem::with_buffer(&data);
    let parser = Parser::with_stream(&stream);
    let message = parser
        .construct_message(None)
        .ok_or_else(|| "no message found".to_string())?;

    let mut found: Vec<Signature> = Vec::new();
    let mut error: Option<String> = None;

    message.foreach(|_, current| {
        if error.is_some() {
            return;
        }
        let Some(signed) = current.dynamic_cast_ref::<gmime::MultipartSigned>() else {
            return;
        };
        match signed.verify(gmime::VerifyFlags::NONE) {
            Ok(Some(signatures)) => {
                for i in 0..signatures.length() {
                    let Some(signature) = signatures.signature(i) else {
                        continue;
                    };
                    found.push(Signature::from_status(
                        signature.status().into_glib(),
                        describe(&signature),
                    ));
                }
            }
            Ok(None) => {}
            Err(e) => error = Some(e.to_string()),
        }
    });
    stream.close();

    match error {
        Some(e) => Err(e),
        None => Ok(found),
    }
}

/// Who the signature says it comes from, falling back to the fingerprint.
fn describe(signature: &gmime::Signature) -> String {
    let Some(certificate) = signature.certificate() else {
        return String::new();
    };
    let name = certificate.name().map(|n| n.to_string()).unwrap_or_default();
    let email = certificate
        .email()
        .map(|e| e.to_string())
        .unwrap_or_default();

    match (name.is_empty(), email.is_empty()) {
        (false, false) => format!("{name} <{email}>"),
        (false, true) => name,
        (true, false) => email,
        (true, true) => certificate
            .fingerprint()
            .map(|f| f.to_string())
            .unwrap_or_default(),
    }
}

/// Points gpg at our keyring for as long as the guard lives.
fn point_gpg_at(home: &Path) -> MutexGuard<'static, ()> {
    let guard = GNUPG.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("GNUPGHOME", home);
    guard
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;

    /// A data folder of its own, with the test key already in it.
    fn with_key() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mailviewer-kde-verify-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let store = KeyStore::open(&dir).unwrap();
        store.import(Path::new("tests/test-key.asc")).unwrap();
        dir
    }

    fn empty() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mailviewer-kde-verify-empty-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reads_a_signature_made_with_a_key_we_hold() {
        let dir = with_key();

        let signatures = verify(Path::new("tests/pgp-signed.eml"), &dir).unwrap();

        assert_eq!(signatures.len(), 1);
        match &signatures[0] {
            Signature::Good { who } | Signature::GoodUnknownKey { who } => {
                assert!(who.contains("test@example.com"), "who: {who}");
            }
            other => panic!("expected a signature that holds, got {other:?}"),
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn says_when_the_message_was_changed() {
        let dir = with_key();

        let signatures = verify(Path::new("tests/pgp-signed-tampered.eml"), &dir).unwrap();

        assert_eq!(signatures.len(), 1);
        assert!(
            matches!(signatures[0], Signature::Bad { .. }),
            "a changed message passed as good: {:?}",
            signatures[0]
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn says_when_the_key_is_not_here() {
        let dir = empty();

        let signatures = verify(Path::new("tests/pgp-signed.eml"), &dir).unwrap();

        assert_eq!(signatures.len(), 1);
        assert!(
            matches!(signatures[0], Signature::NoKey { .. }),
            "expected a missing key, got {:?}",
            signatures[0]
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn says_nothing_about_a_message_without_a_signature() {
        let dir = with_key();

        assert!(verify(Path::new("tests/pgp-encrypted.eml"), &dir)
            .unwrap()
            .is_empty());

        fs::remove_dir_all(dir).unwrap();
    }
}
