/* crypto.rs
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

//! Checking a signature against the keyring of the application.
//!
//! gmime does the work, which matters: getting the bytes that were signed back
//! byte for byte is the hard part of PGP/MIME, and doing it by hand is how
//! verification quietly starts saying "bad signature" for well formed mail.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Mutex, MutexGuard};

use gmime::prelude::Cast;
use gmime::traits::{
    CertificateExt, DataWrapperExt, MessageExt, MultipartExt, MultipartSignedExt, ParserExt,
    PartExt, SignatureExt, SignatureListExt, StreamExt, StreamMemExt,
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

    /// The passphrase of the key the tests below make for themselves.
    const PASSPHRASE: &str = "correct horse battery staple";
    const SECRET: &str = "El secreto es que no hay secreto.";

    /// A keyring with a key that has a passphrase, made here rather than
    /// carried in the repository: a key made by one version of gpg is not
    /// always one another version can open, and this way the gpg that runs
    /// the test is the one that made it. It also keeps a private key out of
    /// the repository.
    fn with_locked_key() -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "mailviewer-kde-decrypt-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let store = KeyStore::open(&dir).unwrap();
        let home = store.home().to_path_buf();

        let made = Command::new("gpg")
            .arg("--homedir")
            .arg(&home)
            .args([
                "--batch",
                "--quiet",
                "--pinentry-mode",
                "loopback",
                "--passphrase",
                PASSPHRASE,
                "--quick-generate-key",
                "MailViewer Locked <locked@example.com>",
                "default",
                "default",
                "never",
            ])
            .output()
            .unwrap();
        assert!(
            made.status.success(),
            "could not make a key: {}",
            String::from_utf8_lossy(&made.stderr)
        );

        let message = dir.join("encrypted.eml");
        fs::write(&message, encrypted_message(&home)).unwrap();
        (dir, message)
    }

    /// A PGP/MIME message encrypted to the key that lives in `home`.
    fn encrypted_message(home: &Path) -> String {
        let inner = "Content-Type: text/plain; charset=utf-8\r\n\r\n{SECRET}\r\n"
            .replace("{SECRET}", SECRET);
        // Tests run side by side, so a name per call rather than per process.
        let plaintext = home.join("inner.txt");
        fs::write(&plaintext, &inner).unwrap();

        let encrypted = Command::new("gpg")
            .arg("--homedir")
            .arg(home)
            .args([
                "--batch",
                "--quiet",
                "--armor",
                "--trust-model",
                "always",
                "--recipient",
                "locked@example.com",
                "--encrypt",
                "--output",
                "-",
            ])
            .arg(&plaintext)
            .output()
            .unwrap();
        let _ = fs::remove_file(&plaintext);
        assert!(
            encrypted.status.success(),
            "could not encrypt: {}",
            String::from_utf8_lossy(&encrypted.stderr)
        );
        let armored = String::from_utf8_lossy(&encrypted.stdout).replace('\n', "\r\n");

        format!(
            "From: MailViewer Locked <locked@example.com>\r\n\
             To: Someone <someone@example.com>\r\n\
             Subject: Lorem ipsum\r\n\
             Date: Mon, 24 Aug 2026 09:00:00 +0200\r\n\
             MIME-Version: 1.0\r\n\
             Content-Type: multipart/encrypted;\r\n\
             \x20protocol=\"application/pgp-encrypted\"; boundary=\"enc-boundary\"\r\n\
             \r\n\
             --enc-boundary\r\n\
             Content-Type: application/pgp-encrypted\r\n\
             \r\n\
             Version: 1\r\n\
             \r\n\
             --enc-boundary\r\n\
             Content-Type: application/octet-stream; name=\"encrypted.asc\"\r\n\
             \r\n\
             {armored}\r\n\
             --enc-boundary--\r\n"
        )
    }

    #[test]
    fn opens_a_message_with_the_right_passphrase() {
        let (dir, message) = with_locked_key();

        let inside = decrypt(&message, &dir, PASSPHRASE).unwrap();
        let inside = String::from_utf8_lossy(&inside);

        assert!(inside.contains(SECRET), "what came out: {inside}");
        assert!(inside.contains("text/plain"), "the headers came along too");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn says_when_the_passphrase_is_wrong() {
        let (dir, message) = with_locked_key();

        let opened = decrypt(&message, &dir, "not the passphrase");

        assert_eq!(opened, Err(DecryptError::WrongPassphrase));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn says_when_the_private_key_is_not_here() {
        let (locked, message) = with_locked_key();
        // A keyring that never saw that key.
        let empty = empty();

        let opened = decrypt(&message, &empty, PASSPHRASE);

        assert_eq!(opened, Err(DecryptError::NoSecretKey));
        fs::remove_dir_all(locked).unwrap();
        fs::remove_dir_all(empty).unwrap();
    }

    #[test]
    fn says_when_there_is_nothing_to_open() {
        let (dir, _) = with_locked_key();

        let opened = decrypt(Path::new("tests/pgp-signed.eml"), &dir, PASSPHRASE);

        assert_eq!(opened, Err(DecryptError::NotEncrypted));
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

/// Why a message could not be opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecryptError {
    /// The passphrase does not open the key.
    WrongPassphrase,
    /// The message was encrypted to a key whose private half is not here.
    NoSecretKey,
    /// Nothing in the message is encrypted.
    NotEncrypted,
    /// gpg said no for some other reason.
    Failed,
}

impl DecryptError {
    pub fn as_str(&self) -> &'static str {
        match self {
            DecryptError::WrongPassphrase => "wrong-passphrase",
            DecryptError::NoSecretKey => "no-secret-key",
            DecryptError::NotEncrypted => "not-encrypted",
            DecryptError::Failed => "failed",
        }
    }
}

/// Opens the encrypted part of the message and gives back what was inside: a
/// mime entity, headers included, ready to be parsed like any message.
///
/// gpg is driven here rather than through gmime because the passphrase has to
/// come from the window: the KDE runtime carries no pinentry, so gpg has
/// nobody to ask.
pub fn decrypt(path: &Path, data_dir: &Path, passphrase: &str) -> Result<Vec<u8>, DecryptError> {
    let blob = encrypted_blob(path).ok_or(DecryptError::NotEncrypted)?;
    let store = KeyStore::open(data_dir).map_err(|_| DecryptError::Failed)?;
    let _guard = point_gpg_at(store.home());

    // The ciphertext goes in a file rather than down the same pipe as the
    // passphrase: gpg reads the passphrase from the first line of stdin and
    // the message from the rest, and how much it takes in one read is not
    // ours to count on. The file holds nothing that is not already encrypted.
    let ciphertext = std::env::temp_dir().join(format!(
        "mailviewer-kde-{}.asc",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::write(&ciphertext, &blob).map_err(|_| DecryptError::Failed)?;

    let mut child = Command::new("gpg")
        .arg("--homedir")
        .arg(store.home())
        .args([
            "--batch",
            "--quiet",
            "--no-tty",
            // The passphrase arrives on a pipe, so no pinentry is needed.
            "--pinentry-mode",
            "loopback",
            "--passphrase-fd",
            "0",
            // Codes rather than prose: what gpg writes for a person is in the
            // language of the machine it runs on.
            "--status-fd",
            "2",
            "--decrypt",
        ])
        .arg(&ciphertext)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| DecryptError::Failed)?;

    // Not ignored: a passphrase that never reaches gpg looks exactly like a
    // wrong one, and that is a bad half hour for whoever debugs it.
    let handed_over = match child.stdin.take() {
        Some(mut stdin) => stdin.write_all(format!("{passphrase}\n").as_bytes()),
        None => Ok(()),
    };

    let output = child.wait_with_output().map_err(|_| DecryptError::Failed)?;
    let _ = std::fs::remove_file(&ciphertext);

    if let Err(e) = handed_over {
        log_write_failure(&e);
        return Err(DecryptError::Failed);
    }

    if output.status.success() && !output.stdout.is_empty() {
        return Ok(output.stdout);
    }

    Err(reason(&String::from_utf8_lossy(&output.stderr)))
}

fn log_write_failure(e: &std::io::Error) {
    eprintln!("the passphrase never reached gpg: {e}");
}

/// Reads the status lines of gpg, the ones written for a program.
///
/// A wrong passphrase and a missing private key both end in NO_SECKEY, since
/// gpg cannot tell a key it may not open from one it does not have. What tells
/// them apart is the number on the ERROR line.
fn reason(status: &str) -> DecryptError {
    // gpg packs the source of the error in the high bits and the code in the
    // low ones. 11 is a bad passphrase, 17 is no secret key.
    const BAD_PASSPHRASE: u32 = 11;
    const NO_SECRET_KEY: u32 = 17;

    let mut found: Option<DecryptError> = None;

    for line in status.lines() {
        let Some(code) = line.strip_prefix("[GNUPG:] ") else {
            continue;
        };
        if code.starts_with("BAD_PASSPHRASE") || code.starts_with("MISSING_PASSPHRASE") {
            return DecryptError::WrongPassphrase;
        }
        if let Some(number) = code.strip_prefix("ERROR ").and_then(gpg_error_code) {
            match number {
                BAD_PASSPHRASE => return DecryptError::WrongPassphrase,
                NO_SECRET_KEY => found = Some(DecryptError::NoSecretKey),
                _ => {}
            }
        }
        if code.starts_with("NO_SECKEY") && found.is_none() {
            found = Some(DecryptError::NoSecretKey);
        }
    }

    found.unwrap_or(DecryptError::Failed)
}

/// "pkdecrypt_failed 67108875" -> 11
fn gpg_error_code(rest: &str) -> Option<u32> {
    let number: u32 = rest.split_whitespace().nth(1)?.parse().ok()?;
    Some(number & 0xFFFF)
}

/// The armored block a PGP/MIME message carries, or the body of one that
/// carries the block inline.
fn encrypted_blob(path: &Path) -> Option<Vec<u8>> {
    let data = std::fs::read(path).ok()?;
    let stream = StreamMem::with_buffer(&data);
    let parser = Parser::with_stream(&stream);
    let message = parser.construct_message(None)?;

    let mut blob: Option<Vec<u8>> = None;
    message.foreach(|_, current| {
        if blob.is_some() {
            return;
        }
        if let Some(encrypted) = current.dynamic_cast_ref::<gmime::MultipartEncrypted>() {
            // Part 0 says which version of the protocol, part 1 is the message.
            if let Some(part) = encrypted.part(1) {
                blob = part_bytes(&part);
            }
        }
    });
    stream.close();

    // The older style puts the block straight in the body of a text part.
    if blob.is_none() && data.windows(27).any(|w| w == b"-----BEGIN PGP MESSAGE-----") {
        return Some(data);
    }
    blob
}

fn part_bytes(object: &gmime::Object) -> Option<Vec<u8>> {
    let part = object.dynamic_cast_ref::<gmime::Part>()?;
    let content = part.content()?;
    let stream = StreamMem::new();
    content.write_to_stream(&stream);
    stream.flush();
    let bytes = stream.byte_array()?;
    Some(bytes.to_vec())
}
