# flluf-lang

[![License: OSL-3.0](https://img.shields.io/badge/license-OSL--3.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange)](https://www.rust-lang.org)
[![Cranelift](https://img.shields.io/badge/cranelift-0.134-purple)](https://github.com/bytecodealliance/wasmtime/tree/main/cranelift)

A minimal compiled language written in Rust with a Cranelift backend.

```
flluf build file.fll   # compile to native executable
flluf run file.fll     # compile and run
```

## Overview

flluf is a language with a clean syntax, static typing, and direct compilation to machine code — no VM, no interpreter, just a binary.

```
use System;

string greet(string name):
{
 return name;
}

void Main():
{
 Log(greet("world"));
 return Exit(0);
}
```

## Types

| Type     | Description              |
| -------- | ------------------------ |
| `int`    | 64-bit signed integer    |
| `float`  | 64-bit floating point    |
| `string` | immutable string         |
| `void`   | no return value          |

## Variables

```
int x = 42;
float pi = 3.14;
string name = "flluf";
```

## Functions

Functions are declared with `returnType name(params):` followed by a block.

`Main` is the entry point. A program without `Main` will not compile.

```
int Add(int a, int b):
{
  return a + b;
}

void Main():
{
  Log(Add(2, 3));
}
```

## Control flow

```
if (x > 0):
{
  Log(x);
}
else if (x == 0):
{
  Log(0);
}
else:
{
  Log(-1);
}
```

Comparison operators: `==`, `!=`, `<`, `>`, `<=`, `>=`.

## Arithmetic

```
a + b   a - b   a * b   a / b
```

Both operands of an arithmetic expression must be the same type (`int` or `float`); mixing them is a compile error.

## Built-in functions

### `Log(value)` / `System::Log(value)`

Prints a value. Works with `int`, `float`, `string`.

Without `use System;` you must write `System::Log(...)`. With `use System;` you can write `Log(...)`.

### `Exit(code)` / `System::Exit(code)`

Terminates the program with the given status code. `0` means success, any
non-zero value means failure. After the program finishes, `flluf run` prints
`program exited with code N` and itself exits with the same status code.

```
return Exit(0);     // success
return Exit(1);     // failure; flluf exits with status 1
```

### `use System`

```
use System;

void Main():
{
  Log(42);             // System:: not needed
  System::Log("hmm");  // still works
}
```

## How it works

1. **Lexer** — tokenizes source into a token stream
2. **Parser** — builds an AST from the token stream
3. **Codegen** — Cranelift IR → native object file
4. **Link** — gcc links the object with a small C runtime

The C runtime provides `printf`-based output (`__flluf_log_i64`, `__flluf_log_f64`, `__flluf_log_ptr`) and wraps `exit()` to report non-zero status codes.

## Building from source

```bash
git clone https://github.com/kazuha046/flluf-lang
cd flluf-lang
cargo build --release
```

Requires Rust 1.85+ and gcc/cc.

## Documentation

The full language documentation is built with [mdBook](https://rust-lang.github.io/mdBook/):

```
cargo install mdbook
mdbook serve docs
```

It is hosted on GitHub Pages at <https://kazuha046.github.io/flluf-lang/>.

## Project structure

```
src/
├── main.rs           # CLI entry point (build / run / new)
├── cli.rs            # clap command definitions
├── build.rs          # compile pipeline + project (init.toml) handling
├── link.rs           # gcc / mingw linking
├── runtime.c         # C runtime for Log and exit codes
├── syntax/           # Frontend: lexer, parser, tokens, AST
│   ├── mod.rs
│   ├── ast.rs
│   ├── lexer.rs
│   ├── parser.rs
│   └── token.rs
├── resolver/         # Module loading, exports, use-scope resolution
│   ├── mod.rs
│   ├── load.rs
│   ├── exports.rs
│   └── scope.rs
├── codegen/          # Cranelift code generation
│   ├── mod.rs
│   ├── compile.rs
│   ├── stmt.rs
│   ├── expr.rs
│   ├── sys.rs
│   └── types.rs
└── modules/          # Standard modules (System.fll)
tests/
└── flluf/            # Integration tests (.fll sources)
└── modules/          # Multi-file project test
```

## License

Open Software License 3.0 License — see [LICENSE](LICENSE)
