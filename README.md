# mailviewer-kde

A Qt6 / KDE frontend for [MailViewer](https://github.com/alescdb/mailviewer), reading
`.eml` and Outlook `.msg` files.

The parsing and the sanitizing are not reimplemented here: they come from
[mailviewer-core](https://github.com/alescdb/mailviewer-core), the same code the GTK
application uses. This side is the window.

## State

Early, but it works: it opens a file given on the command line and shows the headers and
the body, rendered by QtWebEngine.

What is wired:

- Headers and body from the core, with the html sanitized by the same allow list.
- The content security policy the core puts in the document, which is what keeps a message
  from reaching the network. Checked against a local server: with the policy in place a
  message with `@import` and `@font-face` makes no requests, and turning "Show remote
  images" on makes exactly the four it should.
- JavaScript off, an off the record profile, and links handed to the system handler only
  when the scheme is http, https or mailto.

What is wired, second half:

- Attachments, with name, type and size, opened with whatever the desktop uses or saved
  somewhere.
- Find in the message, export as PDF, and print through the system dialog.
- Zoom with the usual shortcuts, remembered between sessions.
- A keyring of its own, in the data folder of the application, with importing and
  removing keys. `~/.gnupg` is never read, and the sandbox is not opened for it.
- Checking the signature of a signed message against that keyring, saying who
  signed it and whether the key is one you certified.
- Translations in Spanish, French, Italian and Dutch, loaded from the locale.
- A desktop entry, an icon and `make install`.

The pages go to the printer as images rendered at 300 dpi, because QtPdf is what
turns the pdf the view writes into something a QPainter can draw.

## Building

```
make            # builds, translations included
make run
sudo make install
```

Needs Qt 6 with QtWebEngine (`qt6-base-dev`, `qt6-declarative-dev`, `qt6-webengine-dev`)
and `libgmime-3.0-dev` for the core, plus `qt6-pdf-dev` for printing and `gnupg`
for the keyring. Install `mold` too: GNU ld drops
libQt6WebEngineQuick from the link and the build fails at the end.

As a flatpak, on the KDE 6.10 runtime:

```
flatpak-builder --user --install build flatpak/io.github.shakaran.mailviewerkde.yml
```

## Why a second frontend

Discussed in [alescdb/mailviewer#48](https://github.com/alescdb/mailviewer/issues/48). The
GTK application runs fine on KDE, so this is not about fixing something broken; it is a
native Qt window and a Chromium renderer for those who want them. It is maintained here,
separately, so nothing lands on the GTK side.
