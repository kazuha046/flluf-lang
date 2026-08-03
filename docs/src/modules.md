# Modules

Code is organized into modules. A module is either a single file
(`module.fll`) or a directory with an `__init__.fll` file (`module/__init__.fll`).
Files inside a directory become submodules of that directory's module.

## `use System`

The `System` module ships with the compiler and provides `Log` and `Exit`.
Nearly every program imports it:

```flluf
use System;

void Main():
{
	Log("Hello, world!");
	return Exit(0);
}
```

## Importing a module

There are several forms of `use`:

```flluf
use module;                // bring a module's functions into scope
use module::*;             // wildcard: all public functions
use module::function;      // one public function
use module::{a, b};        // several named functions
```

For example, given a module `greeter.fll`:

```flluf
pub void hello():
{
	Log("hi");
}
```

it can be used in three equivalent ways:

```flluf
use greeter::hello;        // call hello() directly

use greeter;               // call hello() directly
                           // (imports every function)

use greeter::*;            // same
```

If `hello` is not marked `pub`, it is private to its module and cannot be
imported.

## Qualified calls

You do not need `use` at all — a function can always be called with a
qualified `module::function` path, and `System` works the same way:

```flluf
void Main():
{
	System::Log("direct");
}
```

## Overloads and imports

When an import name matches an overloaded function, **all** overloads with that
name are imported, not just the first one:

```flluf
use module::log;           // imports every `log` overload

void Main():
{
	log(1);                // int overload
	log("text");           // string overload
}
```

## `pub` and re-exports

Anything a module wants to share must be marked `pub`: functions, variables,
and `use` statements alike. A `pub use` inside a module re-exports an imported
item to its own importers:

```flluf
# lib.fll
pub use other::helper;
```

## Directories and `__init__.fll`

A directory module's `__init__.fll` can re-export its submodules:

```flluf
# mylib/__init__.fll
pub use sub::*;

# mylib/sub.fll
pub void helper():
{
	Log("ok");
}
```

```flluf
# main.fll
use mylib::*;

void Main():
{
	helper();
}
```

`pub use` cascades, so a directory module can expose deeper nesting to its
importers.

## Global variables across modules

Globals are reached with a qualified path regardless of how the module was
imported:

```flluf
# sub.fll
pub string greeting = "hi";

# main.fll
use sub::*;

void Main():
{
	Log(sub::greeting);   // hi
}
```
