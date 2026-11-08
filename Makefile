PREFIX ?= $(HOME)/.local
DESTDIR ?=
CARGO ?= cargo
CARGO_FLAGS ?= --locked --offline
APP_ID := io.github.arash.mdview
ICON_SIZES := 16 24 32 48 64 128 256 512
STAGED_PREFIX := $(DESTDIR)$(PREFIX)

.PHONY: build install uninstall refresh-caches

build:
	$(CARGO) build --release $(CARGO_FLAGS)

install: build
	install -Dm755 target/release/nameh "$(STAGED_PREFIX)/bin/nameh"
	install -Dm644 data/$(APP_ID).desktop "$(STAGED_PREFIX)/share/applications/$(APP_ID).desktop"
	@set -e; for size in $(ICON_SIZES); do \
	  install -Dm644 "data/icons/hicolor/$${size}x$${size}/apps/$(APP_ID).png" "$(STAGED_PREFIX)/share/icons/hicolor/$${size}x$${size}/apps/$(APP_ID).png"; \
	done
	rm -f "$(STAGED_PREFIX)/share/icons/hicolor/scalable/apps/$(APP_ID).svg"
	install -Dm644 data/mime/packages/$(APP_ID).xml "$(STAGED_PREFIX)/share/mime/packages/$(APP_ID).xml"
	install -Dm644 README.md "$(STAGED_PREFIX)/share/doc/nameh/README.md"
	install -Dm644 docs/USER_GUIDE.md "$(STAGED_PREFIX)/share/doc/nameh/USER_GUIDE.md"
	install -Dm644 LICENSE "$(STAGED_PREFIX)/share/licenses/nameh/LICENSE"
	$(MAKE) refresh-caches

uninstall:
	@set -e; for size in $(ICON_SIZES); do \
	  rm -f "$(STAGED_PREFIX)/share/icons/hicolor/$${size}x$${size}/apps/$(APP_ID).png"; \
	done
	rm -f "$(STAGED_PREFIX)/bin/nameh" \
	  "$(STAGED_PREFIX)/share/applications/$(APP_ID).desktop" \
	  "$(STAGED_PREFIX)/share/icons/hicolor/scalable/apps/$(APP_ID).svg" \
	  "$(STAGED_PREFIX)/share/mime/packages/$(APP_ID).xml" \
	  "$(STAGED_PREFIX)/share/doc/nameh/README.md" \
	  "$(STAGED_PREFIX)/share/doc/nameh/USER_GUIDE.md" \
	  "$(STAGED_PREFIX)/share/licenses/nameh/LICENSE"
	$(MAKE) refresh-caches

refresh-caches:
	set -e; if test -z "$(DESTDIR)"; then \
	  if command -v update-desktop-database >/dev/null && test -d "$(PREFIX)/share/applications"; then update-desktop-database "$(PREFIX)/share/applications"; fi; \
	  if command -v update-mime-database >/dev/null && test -d "$(PREFIX)/share/mime"; then update-mime-database "$(PREFIX)/share/mime"; fi; \
	  if command -v gtk-update-icon-cache >/dev/null && test -f "$(PREFIX)/share/icons/hicolor/index.theme"; then gtk-update-icon-cache -f "$(PREFIX)/share/icons/hicolor"; fi; \
	fi
