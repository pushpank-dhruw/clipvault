# From-source install for clipvault (daemon + Quickshell frontend).
#
#   make install                 # user install to ~/.local + ~/.config
#   make install PREFIX=/usr/local   # system install (needs sudo)
#   make uninstall

PREFIX ?= $(HOME)/.local
BIN := $(PREFIX)/bin/clipvault
QS_DIR ?= $(HOME)/.config/quickshell/clipvault
UNIT_DIR ?= $(HOME)/.config/systemd/user

.PHONY: build install uninstall run test lint

build:
	cargo build --release

install: build
	install -Dm755 target/release/clipvault "$(BIN)"
	install -dm755 "$(QS_DIR)"
	install -m644 quickshell/*.qml "$(QS_DIR)/"
	install -dm755 "$(UNIT_DIR)"
	sed 's|^ExecStart=.*|ExecStart=$(BIN)|' packaging/clipvault.service > "$(UNIT_DIR)/clipvault.service"
	@echo ""
	@echo "clipvault installed to $(BIN)"
	@echo "  1. ensure $(PREFIX)/bin is on your PATH"
	@echo "  2. systemctl --user enable --now clipvault"
	@echo "  3. add to ~/.config/hypr/hyprland.conf:"
	@echo "       exec-once = qs -c clipvault"
	@echo "       bind = SUPER SHIFT, V, exec, clipvault toggle"

uninstall:
	rm -f "$(BIN)" "$(UNIT_DIR)/clipvault.service"
	rm -rf "$(QS_DIR)"

run: build
	./target/release/clipvault

test:
	cargo test

lint:
	cargo clippy --all-targets -- -D warnings
	qmllint -I /usr/lib/qt6/qml quickshell/Shelf.qml quickshell/Card.qml quickshell/HotZone.qml quickshell/Settings.qml
