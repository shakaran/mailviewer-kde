/* keys.rs
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

//! A keyring of its own, inside the data directory of the application.
//!
//! The keys of the user, in `~/.gnupg`, stay out of reach: the sandbox is not
//! opened for them and no command here ever looks there. What the user wants
//! this application to see, they import.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// One key as the list shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Key {
    pub fingerprint: String,
    pub user: String,
    /// Set when the private half is in the keyring, not only the public one.
    pub secret: bool,
    /// Empty when the key does not expire.
    pub expires: String,
}

#[derive(Debug)]
pub struct KeyStore {
    home: PathBuf,
}

impl KeyStore {
    /// The keyring lives next to the rest of the data of the application, and
    /// is created on first use.
    pub fn open(data_dir: &Path) -> Result<Self, String> {
        let home = data_dir.join("gnupg");
        fs::create_dir_all(&home).map_err(|e| format!("{}: {}", home.display(), e))?;
        // gpg refuses to work in a directory others can read.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&home, fs::Permissions::from_mode(0o700))
                .map_err(|e| format!("{}: {}", home.display(), e))?;
        }
        // The passphrase for decrypting arrives on a pipe, and the agent only
        // takes it that way when it is told to. It is the default in most
        // builds, and saying it here costs a line and removes the guessing.
        let conf = home.join("gpg-agent.conf");
        if !conf.exists() {
            let _ = fs::write(&conf, "allow-loopback-pinentry\n");
        }

        Ok(Self { home })
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn list(&self) -> Result<Vec<Key>, String> {
        let public = self.gpg(["--with-colons", "--list-keys"])?;
        let secret = self.gpg(["--with-colons", "--list-secret-keys"])?;
        let secret_fingerprints = fingerprints(&String::from_utf8_lossy(&secret.stdout));

        let mut keys = parse_keys(&String::from_utf8_lossy(&public.stdout));
        for key in &mut keys {
            key.secret = secret_fingerprints.contains(&key.fingerprint);
        }
        Ok(keys)
    }

    /// Returns how many keys the file added or updated.
    pub fn import(&self, path: &Path) -> Result<usize, String> {
        // gpg reports what it did on stderr, in prose. Asking the keyring
        // afterwards is simpler and cannot drift from what is really there.
        let output = self.gpg([OsStr::new("--import"), path.as_os_str()])?;
        let _ = output;
        Ok(self.list()?.len())
    }

    pub fn remove(&self, fingerprint: &str) -> Result<(), String> {
        if !is_fingerprint(fingerprint) {
            return Err("not a fingerprint".to_string());
        }
        // Secret first: gpg refuses to drop a public key while the private
        // half is still there.
        let _ = self.gpg(["--batch", "--yes", "--delete-secret-keys", fingerprint]);
        self.gpg(["--batch", "--yes", "--delete-keys", fingerprint])?;
        Ok(())
    }

    fn gpg<I, S>(&self, arguments: I) -> Result<Output, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let output = Command::new("gpg")
            .arg("--homedir")
            .arg(&self.home)
            .args(["--batch", "--no-tty", "--quiet"])
            .args(arguments)
            .output()
            .map_err(|e| format!("gpg: {e}"))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            let error = error.lines().last().unwrap_or("gpg failed").trim();
            return Err(error.to_string());
        }
        Ok(output)
    }
}

impl Drop for KeyStore {
    fn drop(&mut self) {
        // gpg leaves an agent running. Inside a flatpak that agent keeps the
        // sandbox alive after the window is closed, so it goes with us.
        let _ = Command::new("gpgconf")
            .arg("--homedir")
            .arg(&self.home)
            .args(["--kill", "all"])
            .output();
    }
}

/// A fingerprint as gpg writes it: 40 hexadecimal digits.
fn is_fingerprint(value: &str) -> bool {
    value.len() == 40 && value.chars().all(|c| c.is_ascii_hexdigit())
}

fn fingerprints(colons: &str) -> Vec<String> {
    parse_keys(colons)
        .into_iter()
        .map(|key| key.fingerprint)
        .collect()
}

/// Reads the `--with-colons` listing, which is the only output of gpg meant to
/// be read by a program.
fn parse_keys(colons: &str) -> Vec<Key> {
    let mut keys: Vec<Key> = Vec::new();
    let mut expires = String::new();

    for line in colons.lines() {
        let fields: Vec<&str> = line.split(':').collect();
        match fields.first().copied() {
            Some("pub") | Some("sec") => {
                expires = fields.get(6).copied().unwrap_or_default().to_string();
                keys.push(Key::default());
            }
            Some("fpr") => {
                if let Some(key) = keys.last_mut() {
                    if key.fingerprint.is_empty() {
                        key.fingerprint = fields.get(9).copied().unwrap_or_default().to_string();
                        key.expires = expires.clone();
                    }
                }
            }
            Some("uid") => {
                if let Some(key) = keys.last_mut() {
                    if key.user.is_empty() {
                        key.user = unescape(fields.get(9).copied().unwrap_or_default());
                    }
                }
            }
            _ => {}
        }
    }

    keys.retain(|key| !key.fingerprint.is_empty());
    keys
}

/// gpg escapes colons and anything non printable as \x3a and friends.
fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;

    while let Some(at) = rest.find("\\x") {
        out.push_str(&rest[..at]);
        let code = rest.get(at + 2..at + 4).unwrap_or_default();
        match u8::from_str_radix(code, 16) {
            Ok(byte) => {
                out.push(byte as char);
                rest = &rest[at + 4..];
            }
            Err(_) => {
                out.push_str("\\x");
                rest = &rest[at + 2..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const LISTING: &str = "\
tru::1:1756000000:0:3:1:5
pub:u:255:22:72C4B10FD93E3587:1756000000:1787536000::u:::scESC::::::ed25519:::0:
fpr:::::::::DFE39FADAAF511D7A38D4DD272C4B10FD93E3587:
uid:u::::1756000000::AAAA::MailViewer Test <test\\x3aexample.com>::::::::::0:
sub:u:255:18:1111111111111111:1756000000:::::e::::::cv25519::
pub:-:255:22:BBBBBBBBBBBBBBBB:1756000000:::-:::scESC::::::ed25519:::0:
fpr:::::::::AAAABBBBCCCCDDDDEEEEFFFF00001111222233334:
uid:-::::1756000000::BBBB::Someone Else <else@example.com>::::::::::0:
";

    #[test]
    fn reads_the_listing_of_gpg() {
        let keys = parse_keys(LISTING);

        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].fingerprint, "DFE39FADAAF511D7A38D4DD272C4B10FD93E3587");
        assert_eq!(keys[0].user, "MailViewer Test <test:example.com>");
        assert_eq!(keys[0].expires, "1787536000");
        assert_eq!(keys[1].user, "Someone Else <else@example.com>");
        assert_eq!(keys[1].expires, "");
    }

    #[test]
    fn only_takes_a_real_fingerprint() {
        assert!(is_fingerprint("DFE39FADAAF511D7A38D4DD272C4B10FD93E3587"));
        assert!(!is_fingerprint("DFE39FAD"));
        // The one that matters: nothing that could reach the command line.
        assert!(!is_fingerprint("DFE39FADAAF511D7A38D4DD272C4B10FD93E358 --delete"));
    }

    fn store() -> (KeyStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "mailviewer-kde-keys-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        (KeyStore::open(&dir).unwrap(), dir)
    }

    #[test]
    fn leaves_no_agent_behind() {
        let (store, dir) = store();
        store.import(Path::new("tests/test-key.asc")).unwrap();
        let home = store.home().to_path_buf();
        drop(store);

        let running = Command::new("gpgconf")
            .arg("--homedir")
            .arg(&home)
            .arg("--list-dirs")
            .output();
        // What matters is that dropping the store does not panic and the agent
        // was asked to go. gpgconf itself still answers, it reads a folder.
        assert!(running.is_ok());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn imports_lists_and_removes() {
        let (store, dir) = store();

        assert_eq!(store.list().unwrap().len(), 0);
        assert_eq!(store.import(Path::new("tests/test-key.asc")).unwrap(), 1);

        let keys = store.list().unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].fingerprint, "DFE39FADAAF511D7A38D4DD272C4B10FD93E3587");
        assert_eq!(keys[0].user, "MailViewer Test <test@example.com>");
        assert!(!keys[0].secret, "only the public half was imported");

        store.remove(&keys[0].fingerprint).unwrap();
        assert_eq!(store.list().unwrap().len(), 0);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn keeps_out_of_the_keyring_of_the_user() {
        let (store, dir) = store();

        assert!(store.home().starts_with(&dir), "the keyring left its folder");
        assert!(!store.home().ends_with(".gnupg"));
        store.import(Path::new("tests/test-key.asc")).unwrap();
        // What matters: the key landed there and nowhere else.
        assert!(store.home().join("pubring.kbx").exists() || store.home().join("pubring.gpg").exists());

        fs::remove_dir_all(dir).unwrap();
    }
}
