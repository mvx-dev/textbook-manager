INSTALL_DIR = "$(HOME)/.local"
EXEC_NAME = "tb-manager"
APP_NAME = "tbm"

default: install

clean:
	@rm -rf target/*
	@cargo clean

check:
	@cargo check

build_release:
	@cargo build --release

build:
	@cargo build

run:
	@cargo run

install_debug: uninstall build
	@cp target/debug/$(EXEC_NAME) $(INSTALL_DIR)/bin/$(APP_NAME)
	@$(INSTALL_DIR)/bin/$(APP_NAME) init

install: uninstall build_release
	@cp target/release/$(EXEC_NAME) $(INSTALL_DIR)/bin/$(APP_NAME)
	@$(INSTALL_DIR)/bin/$(APP_NAME) init

clean_install: install clean

uninstall:
	@rm -rf $(INSTALL_DIR)/bin/$(APP_NAME)
