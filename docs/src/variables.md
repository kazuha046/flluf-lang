# Variables

Variables are immutable by default and must be given a type.

```flluf
void Main():
{
	int count = 3;
	string name = "flluf";
}
```

## Mutability

Reassignment is only allowed when the variable is declared with `mut`:

```flluf
mut int x = 1;
x = 2;          // OK
Log(x);         // prints 2
```

Without `mut`, reassignment is a compile-time error:

```flluf
int y = 1;
y = 2;          // Error: cannot assign to immutable variable `y`
```

The same rule applies to `mut` parameters (see
[Functions](functions.md#parameters)) and `mut` block bindings (see
[Control Flow](control-flow.md#if)).

## Global variables

Globals are declared at the top level of a module, outside any function:

```flluf
pub string greeting = "hi from util";
```

The type and the initializer are both required. To make a global visible to
other modules it must be marked `pub` (see [Modules](modules.md#pub-export)).

Globals are read-only and are accessed with a qualified `module::name` path
rather than a bare name:

```flluf
use System;
use sub::*;

void Main():
{
	Log(sub::greeting);
}
```

```text
hi from util
```

Note that a bare reference like `Log(greeting)` will *not* resolve to a global —
bare names always mean local variables, so that code fails to compile.
