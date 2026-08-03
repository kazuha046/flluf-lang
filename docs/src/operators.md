# Operators and Expressions

## Arithmetic

The binary arithmetic operators `+`, `-`, `*`, and `/` work on `int` and on
`float`. Integer division truncates toward zero:

```flluf
Log(10 / 4);      // 2
Log(10.0 / 4.0);  // 2.5
```

The usual precedence applies: `*` and `/` bind tighter than `+` and `-`.

```flluf
Log(7 + 2 * 3);   // 13
```

There is no `%` (modulo) operator.

### Mixed types

Both operands must be the **same** type. Mixing an `int` and a `float` in an
arithmetic expression is a compile-time error:

```flluf
Log(2.5 + 1);   // Error: type mismatch: Float vs Int
```

### Strings are not arithmetic

Strings cannot be combined with the arithmetic operators:

```flluf
Log("a" + "b");   // Error: cannot use string in arithmetic
```

There is no string concatenation operator.

## Comparisons

The comparison operators `==`, `!=`, `<`, `>`, `<=`, and `>=` work on numbers
of the same type and produce an `int` result: `1` for true, `0` for false.

```flluf
Log(1 == 2);    // 0
Log(1 != 2);    // 1
Log(5 < 2);     // 0
Log(5 >= 5);    // 1
```

Comparisons are commonly used as conditions in `if` statements (see
[Control Flow](control-flow.md)).

## Where `int -> float` conversion is allowed

An `int` can be used where a `float` is expected in exactly two places:

1. **Assignment** — the value is converted:

   ```flluf
	float pi = 3;   // OK, pi is 3.000000
   ```

2. **Function-call arguments** — an `int` argument is converted to match a
   `float` parameter (and vice versa):

   ```flluf
	void show(float x):
	{
		Log(x);
	}

	void Main():
	{
		show(2);    // prints 2.000000
	}
   ```

Nothing else converts implicitly: `return 2;` from a `float` function, mixing
types in arithmetic, or converting a `float` to an `int` by assignment are all
errors.
