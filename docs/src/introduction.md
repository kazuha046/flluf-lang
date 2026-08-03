# Introduction

**flluf** is a minimal compiled language written in Rust with a
[Cranelift](https://github.com/bytecodealliance/wasmtime/tree/main/cranelift) backend.

It compiles directly to native machine code — there is no VM and no interpreter,
just an executable.

```
use System;

string greet(string name):
{
	return "hello, " + name;
}

void Main():
{
	Log(greet("world"));
	return Exit(0);
}
```

## Design goals

- **Small and simple.** The language fits comfortably in a single afternoon of
  reading. There are no classes, no closures, no generics — just types,
  functions, variables, and modules.
- **Statically typed.** Every variable, parameter, and return value has a fixed
  type, checked at compile time.
- **Native performance.** Code is lowered straight to machine code through
  Cranelift, so it runs at native speed with no runtime overhead.

## Feature overview

- Primitive types: `int`, `float`, `string`, and `void`
- Immutable-by-default variables and parameters, opt-in mutability with `mut`
- Functions with multiple return types, plus function overloading
- `if` / `else if` / `else` control flow
- A module system with `use` statements, `pub` exports, and re-exports
- A `System` module with `Log` and `Exit`
- Clear, position-aware compiler diagnostics
