PREFIX ?= /usr
DESTDIR ?=
QT_LRELEASE ?= /usr/lib/qt6/bin/lrelease

all: translations
	cargo build --release

translations:
	$(QT_LRELEASE) i18n/*.ts

run:
	cargo run -- sample.eml

install: all
	install -Dm755 target/release/mailviewer-kde $(DESTDIR)$(PREFIX)/bin/mailviewer-kde
	install -Dm644 data/io.github.shakaran.mailviewerkde.desktop \
		$(DESTDIR)$(PREFIX)/share/applications/io.github.shakaran.mailviewerkde.desktop
	install -Dm644 data/io.github.shakaran.mailviewerkde.svg \
		$(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/io.github.shakaran.mailviewerkde.svg
	for qm in i18n/*.qm; do \
		install -Dm644 $$qm $(DESTDIR)$(PREFIX)/share/mailviewer-kde/translations/$$(basename $$qm); \
	done

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/mailviewer-kde
	rm -f $(DESTDIR)$(PREFIX)/share/applications/io.github.shakaran.mailviewerkde.desktop
	rm -f $(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/io.github.shakaran.mailviewerkde.svg
	rm -rf $(DESTDIR)$(PREFIX)/share/mailviewer-kde

clean:
	cargo clean
	rm -f i18n/*.qm

.PHONY: all translations run install uninstall clean
