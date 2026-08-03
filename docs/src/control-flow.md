# Control Flow

## `if`

An `if` statement runs a block when its condition is true. Conditions are
expressions that produce an `int` — non-zero means true, zero means false.
Typically you write a comparison:

```flluf
use System;

void Main():
{
	int x = 10;

	if (x > 5):
	{
		Log("big");
	}
}
```

The condition is written in parentheses and is followed by a colon. Every
branch is a block delimited by curly braces:

```flluf
if (x < 0):
{
	Log("negative");
}
else if (x == 0):
{
	Log("zero");
}
else:
{
	Log("positive");
}
```

A full `else if` chain is supported. Blocks introduce a new scope — variables
declared inside a block are not visible outside it:

```flluf
if (true):
{
	int hidden = 1;
}

Log(hidden);   // Error: undefined variable `hidden`
```

## No loops (yet)

flluf currently has no `while`, `for`, or `loop` constructs. Repetition is only
possible through recursion:

```flluf
void count_down(int n):
{
	Log(n);

	if (n > 0):
	{
		count_down(n - 1);
	}
}
```

## `return`

`return` ends the current function early and optionally yields a value. The
value must match the function's return type (see [Functions](functions.md)):

```flluf
void Main():
{
	if (1 != 2):
	{
		return Exit(1);
	}

	Log("never reached");
}
```
