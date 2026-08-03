# Projects

A flluf project is a directory containing an `init.toml` manifest and a
`main.fll` entry point.

## Layout

```
hello/
├── init.toml
└── main.fll
```

## `init.toml`

```toml
[package]
name = "hello"        # the name of the produced binary
version = "1.0.0"     # metadata only
```

- **`name`** — controls the name of the compiled executable.
- **`version`** — informational only; it has no effect on compilation.

The manifest is optional in a sense — the compiler falls back to sensible
defaults — but `flluf new` always generates one, and tests in the repository
ship with it.

## `main.fll`

The entry point must define `Main`:

```flluf
use System;

void Main():
{
	Log("Hello, world!");
}
```

## Modules inside a project

Put module files and directories next to `main.fll` and import them with `use`:

```
project/
├── init.toml
├── main.fll
├── util.fll
└── lib/
    └── __init__.fll
```

```flluf
# main.fll
use util;
use lib::*;
```

## `flluf new`

Create a ready-to-run project from the current directory:

```sh
flluf new hello
cd hello
```

This writes `init.toml` and a `Hello, World!` `main.fll`, then fails if the
directory already exists.
