# Ekphos Makefile
# A lightweight, terminal-based markdown research tool

BINARY_NAME := ekphos
VERSION := 0.10.0
BUILD_DIR := target
RELEASE_DIR := $(BUILD_DIR)/release
DEBUG_DIR := $(BUILD_DIR)/debug

# Installation directories
PREFIX ?= /usr/local
BINDIR := $(PREFIX)/bin
MANDIR := $(PREFIX)/share/man/man1
COMPLETIONS_DIR := $(PREFIX)/share/bash-completion/completions

# Platform detection
UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Darwin)
    PLATFORM := macos
endif
ifeq ($(UNAME_S),Linux)
    PLATFORM := linux
endif

.PHONY: all build release debug clean install uninstall help test check fmt lint run

# Default target
all: release

# Build release version
release:
	@echo "Building release version..."
	cargo build --release
	@echo "Binary available at $(RELEASE_DIR)/$(BINARY_NAME)"

# Build debug version
debug:
	@echo "Building debug version..."
	cargo build
	@echo "Binary available at $(DEBUG_DIR)/$(BINARY_NAME)"

# Alias for release
build: release

# Run the application
run:
	cargo run

# Run in release mode
run-release:
	cargo run --release

# Run tests
test:
	cargo test

# Check code without building
check:
	cargo check

# Format code
fmt:
	cargo fmt

# Format check (CI)
fmt-check:
	cargo fmt -- --check

# Lint with clippy
lint:
	cargo clippy -- -D warnings

# Clean build artifacts
clean:
	cargo clean
	@echo "Cleaned build artifacts"

# Install binary to system
install: release
	@echo "Installing $(BINARY_NAME) to $(BINDIR)..."
	@mkdir -p $(BINDIR)
	@cp $(RELEASE_DIR)/$(BINARY_NAME) $(BINDIR)/$(BINARY_NAME)
	@chmod 755 $(BINDIR)/$(BINARY_NAME)
	@echo "Installation complete!"
	@echo "Run '$(BINARY_NAME)' to start"

# Uninstall binary from system
uninstall:
	@echo "Uninstalling $(BINARY_NAME)..."
	@rm -f $(BINDIR)/$(BINARY_NAME)
	@echo "Uninstallation complete!"

# Install for current user only
install-user: release
	@echo "Installing $(BINARY_NAME) to ~/.local/bin..."
	@mkdir -p ~/.local/bin
	@cp $(RELEASE_DIR)/$(BINARY_NAME) ~/.local/bin/$(BINARY_NAME)
	@chmod 755 ~/.local/bin/$(BINARY_NAME)
	@echo "Installation complete!"
	@echo "Make sure ~/.local/bin is in your PATH"

# Uninstall from user directory
uninstall-user:
	@rm -f ~/.local/bin/$(BINARY_NAME)
	@echo "Uninstalled from ~/.local/bin"

# Create distribution tarball
dist: release
	@echo "Creating distribution tarball..."
	@mkdir -p dist
	@tar -czvf dist/$(BINARY_NAME)-$(VERSION)-$(PLATFORM)-$(shell uname -m).tar.gz \
		-C $(RELEASE_DIR) $(BINARY_NAME)
	@echo "Distribution tarball created at dist/"

# Package for Debian/Ubuntu (.deb)
deb: release
	@echo "Creating .deb package..."
	@mkdir -p pkg/deb/$(BINARY_NAME)_$(VERSION)_amd64/DEBIAN
	@mkdir -p pkg/deb/$(BINARY_NAME)_$(VERSION)_amd64/usr/bin
	@cp $(RELEASE_DIR)/$(BINARY_NAME) pkg/deb/$(BINARY_NAME)_$(VERSION)_amd64/usr/bin/
	@echo "Package: $(BINARY_NAME)" > pkg/deb/$(BINARY_NAME)_$(VERSION)_amd64/DEBIAN/control
	@echo "Version: $(VERSION)" >> pkg/deb/$(BINARY_NAME)_$(VERSION)_amd64/DEBIAN/control
	@echo "Section: utils" >> pkg/deb/$(BINARY_NAME)_$(VERSION)_amd64/DEBIAN/control
	@echo "Priority: optional" >> pkg/deb/$(BINARY_NAME)_$(VERSION)_amd64/DEBIAN/control
	@echo "Architecture: amd64" >> pkg/deb/$(BINARY_NAME)_$(VERSION)_amd64/DEBIAN/control
	@echo "Maintainer: Ekphos Contributors" >> pkg/deb/$(BINARY_NAME)_$(VERSION)_amd64/DEBIAN/control
	@echo "Description: Ekphos - A lightweight terminal-based markdown research tool" >> pkg/deb/$(BINARY_NAME)_$(VERSION)_amd64/DEBIAN/control
	@dpkg-deb --build pkg/deb/$(BINARY_NAME)_$(VERSION)_amd64
	@mv pkg/deb/$(BINARY_NAME)_$(VERSION)_amd64.deb dist/ 2>/dev/null || true
	@echo "Package created at dist/$(BINARY_NAME)_$(VERSION)_amd64.deb"

# Package for RPM-based distros
rpm: release
	@echo "Creating .rpm package requires rpmbuild..."
	@echo "Use 'cargo install cargo-rpm' and 'cargo rpm build' instead"

# Show help
help:
	@echo "Ekphos - A lightweight terminal-based markdown research tool"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@echo "Targets:"
	@echo "  all          Build release version (default)"
	@echo "  release      Build optimized release binary"
	@echo "  debug        Build debug binary"
	@echo "  build        Alias for release"
	@echo "  run          Run in debug mode"
	@echo "  run-release  Run in release mode"
	@echo "  test         Run tests"
	@echo "  check        Check code without building"
	@echo "  fmt          Format code with rustfmt"
	@echo "  lint         Lint code with clippy"
	@echo "  clean        Remove build artifacts"
	@echo "  install      Install to $(BINDIR) (requires sudo)"
	@echo "  uninstall    Remove from $(BINDIR) (requires sudo)"
	@echo "  install-user Install to ~/.local/bin"
	@echo "  uninstall-user Remove from ~/.local/bin"
	@echo "  dist         Create distribution tarball"
	@echo "  deb          Create .deb package"
	@echo "  help         Show this help message"
	@echo ""
	@echo "Variables:"
	@echo "  PREFIX       Installation prefix (default: /usr/local)"
	@echo ""
	@echo "Examples:"
	@echo "  make                    # Build release"
	@echo "  make install            # Install system-wide"
	@echo "  sudo make install       # Install with root"
	@echo "  make PREFIX=~/.local install  # Install to home"
