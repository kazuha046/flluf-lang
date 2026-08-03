# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2026-08-03

### Added

- Positioned compiler errors: every error now reports `file:line:col` with a source
  snippet and a caret marker across the lexer, parser, resolver, and codegen.
- Strict static type checking for variable declarations, assignments, and return
  values. `int -> float` widening is the only implicit conversion; every other
  mismatch is a compile error with a position.

### Changed

- Imports are now scoped per module — a `use` in one module no longer leaks into
  others, so `Log` again requires `use System;`.
- Imports declared in an `__init__.fll` file now cascade into its submodules.
- The binary name is derived from `[package] name` in `init.toml` via the `toml`
  crate instead of a hand-rolled parser.
- The resolver was reorganized into `load`, `exports`, and `scope` modules.

### Fixed

- `Log` (and other imported symbols) no longer leak between modules.
- Compiler errors now point at the exact file, line, and column instead of a bare message.

## [0.2.0] - 2026-07-31

Version bump release; no functional changes.

## [0.1.0] - 2026-07-29

### Added

- Initial release of the flluf compiler.
- Native executable compilation through a Cranelift backend — no VM, no interpreter.
- Statically typed language with `int`, `float`, `string`, and `void` types.
- CLI: `flluf build <file>`, `flluf run <file>`, and `flluf new <name>`.
- Project builds rooted at `main.fll` via `flluf build <dir>` / `flluf run <dir>`.
- Project scaffolding through `init.toml`; `[package] name` becomes the binary name.
- Module system with `use` statements, `pub` exports, re-exports, and function overloads.
- `System` module exposing `Log(int | float | string)` and `Exit(int)`.
- Exit-code logging after running a program.
- Windows build support (MSVC `link.exe` and MinGW cross-compilation).
- Nix flake development shell and GitHub Actions CI workflow.

[0.3.0]: https://github.com/kazuha046/flluf-lang/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/kazuha046/flluf-lang/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/kazuha046/flluf-lang/releases/tag/v0.1.0
