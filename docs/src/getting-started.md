# Getting Started

## Requirements

- Rust 1.85 or newer (`cargo`, `rustc`)
- A C compiler on the PATH:
  - Linux / macOS: `gcc` or `cc`
  - Windows: `link.exe` (Visual Studio), or MinGW (`x86_64-w64-mingw32-gcc`)

## Building the compiler

```sh
git clone git@github.com:kazuha046/flluf-lang.git
cd flluf-lang
cargo build --release
```

The `flluf` binary is written to `target/release/flluf`. Add it to your `PATH`
or call it by its full path:

```sh
alias flluf="$PWD/target/release/flluf"
```

## Your first program

Scaffold a new project:

```sh
flluf new hello
cd hello
```

This creates:

```
hello/
├── init.toml   # project metadata
└── main.fll    # entry point with a Hello, World! example
```

Build and run it:

```sh
flluf run .
```

You should see:

```
Hello, world!
program exited with code 0
```

The executable is written to `target/hello` next to your source.
