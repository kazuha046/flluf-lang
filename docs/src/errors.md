# Error Reporting

Compiler errors point at the exact source position with a caret:

```text
error: undefined variable `limit`
      --> main.fll:8:1
         |
       8 | 	Log(limit);
         | ^
```

Every message includes the file, line, and column, the offending line, and a
caret marking the exact character range that caused the problem.

## Common errors

### Undefined variables and functions

```flluf
Log(nope);   // error: undefined variable `nope`
```

### Immutable assignment

```flluf
int y = 1;
y = 2;       // error: cannot assign to immutable variable `y`
```

Mark the variable `mut` to fix it.

### Type mismatch

Arithmetic requires both operands to be the same type:

```flluf
Log(2.5 + 1);   // error: type mismatch: Float vs Int
```

Assigning an incompatible value to a variable or return value:

```flluf
string s = 42;   // error: cannot use int as string
```

### Missing `use`

Calling a `System` function without importing it first:

```flluf
void Main():
{
	Log("hi");   // error: 'Log' requires 'use System;'
}
```

Add `use System;` at the top of the file.

### Missing `return`

```flluf
int broken():
{
	Log("no return");
}   // error: function `broken` must return a value of type `int`
```

### Import errors

Importing something a module does not export:

```flluf
use greeter::missing;   // error: `missing` is not exported by module `greeter`
```

Modules must mark items `pub` for them to be importable (see
[Modules](modules.md#pub-and-re-exports)).
