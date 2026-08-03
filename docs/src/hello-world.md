# Hello, World!

```flluf
use System;

void Main():
{
	Log("Hello, world!");
	return Exit(0);
}
```

## The `Main` entry point

Every program must define exactly one function named `Main` with no parameters.
It is the first thing that runs when you start the executable.

```
void Main():
```

`Main` has no return type — a return value is optional. `return Exit(code)` is a
convenient way to both exit and report a status code.

## `Log`

`Log` is provided by the `System` module and prints a value followed by a newline:

```flluf
use System;

void Main():
{
	Log(42);
	Log(3.14);
	Log("text");
}
```

`Log` accepts `int`, `float`, or `string`. As a quirk of its implementation, a
`Log(...)` call *evaluates to its first argument*, so it can be used in
expressions:

```flluf
string echo(string s):
{
	return Log(s);
}
```

## `Exit`

`Exit` terminates the program with the given status code. `0` means success,
any non-zero value means failure.

```flluf
void Main():
{
	if (1 != 2):
	{
		return Exit(1);
	}
}
```

The shell reports the code after the program finishes:

```
program exited with code 1
```

Calling `Exit` is optional — if `Main` ends normally, the program exits with
code `0`.
