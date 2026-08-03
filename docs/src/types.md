# Types

flluf has four primitive types:

| Type     | Meaning                              | Size/representation        |
|----------|--------------------------------------|----------------------------|
| `int`    | Signed 64-bit integer                | 64 bits                    |
| `float`  | IEEE-754 double-precision float      | 64 bits                    |
| `string` | Immutable, NUL-terminated byte string| pointer                    |
| `void`   | No value (functions only)            | —                          |

## Literals

```flluf
int a = 42;
int b = -7;

float c = 3.14;
float d = 2.0;

string s = "hello, world";
```

Integer and floating-point literals are written the usual way. String literals
are double-quoted.

## Type safety

flluf is strictly typed. A value can only be used where its type matches what
is expected:

```flluf
int x = 1;
string s = x;   // Error: cannot use int as string
```

```flluf
string s = "hi";
int x = s;      // Error: cannot use string as int
```

### Widening

The one implicit conversion allowed is `int -> float`. Integers widen to floats
in assignments and in arithmetic (see
[Operators](operators.md#arithmetic-and-int-float-promotion)):

```flluf
float pi = 3;       // OK, prints 3.000000
```

Everything else must match exactly. An `int` cannot become a `string`, a
`float` cannot become an `int`, and so on.

## `void`

`void` is used only as a function return type to mean "returns nothing":

```flluf
void greet():
{
	Log("hi");
}
```

You cannot declare a `void` variable.
