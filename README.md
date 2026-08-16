# Ekphos

[![Crates.io](https://img.shields.io/crates/v/ekphos)](https://crates.io/crates/ekphos)
[![Rust](https://img.shields.io/badge/rust-1.86%2B-orange)](https://www.rust-lang.org/)
[![License](https://img.shields.io/crates/l/ekphos)](https://github.com/hanebox/ekphos/blob/main/LICENSE)

A lightweight, fast, terminal-based markdown research tool built with Rust.

![Ekphos Preview](examples/ekphos-screenshot.png)

## Documentation

**Go to [Documentation](https://ekphos.netlify.app/docs)**

## Quick Start

To install with [Cargo](https://doc.rust-lang.org/cargo/):

```bash
cargo install ekphos
```

Alternatively, you can install Ekphos using [Homebrew](https://brew.sh):

```bash
brew install ekphos
```

Or using [AUR](https://aur.archlinux.org/packages/ekphos):

```bash
yay -S ekphos
```

_Note: Always update to the latest version. If you encounter config issues after updating, run `ekphos --reset` to reset your configuration._

## Requirements

- Rust 1.86+
- For inline images: iTerm2, Kitty, WezTerm, Ghostty, or Sixel-compatible terminal

## Discussion

- Open a discussion in the [repository](https://github.com/hanebox/ekphos/discussions)

## Disclaimer

This project is in early development. There may be breaking changes and bugs in pre-releases.

## Contributing

```bash
git clone https://github.com/hanebox/ekphos.git
cd ekphos
```

1. Fork the repository
2. Create a feature branch from `main`
3. Make your changes
4. Submit a PR to the `main` branch

Run the workspace checks before submitting:

```bash
cargo test --workspace --all-targets --locked
scripts/check-crate-boundaries.sh
scripts/clippy-ratchet.sh
```

The root crate owns terminal/application integration. Core, vault, editor, Vim,
search, and graph code live in independently testable crates under `crates/`.

To contribute to the documentation, see [ekphos-docs](https://github.com/hanebox/ekphos-docs).

[![Packaging status](https://repology.org/badge/vertical-allrepos/ekphos.svg)](https://repology.org/project/ekphos/versions)

## License

MIT
