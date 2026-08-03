# Functions

## Declaring a function

A function is declared with a return type, a name, parameters, and a block:

```flluf
int add(int a, int b):
{
	return a + b;
}
```

Every function must end by returning a value that matches its declared type —
if control could reach the end of the function without a `return`, compilation
fails.

```flluf
int broken():
{
	Log("no return");
}   // Error: function `broken` must return a value of type `int`
```

The one exception is `void` functions, which return nothing and may end
normally:

```flluf
void greet():
{
	Log("hi");
}
```

`return` is also used to exit early from a `void` function:

```flluf
void greet(string name):
{
	if (name == ""):
	{
		return;
	}

	Log(name);
}
```

## Parameters

Parameters are typed and, like local variables, are immutable by default. Mark
one with `mut` to reassign it inside the function body:

```flluf
int clamp(int x):
{
	mut int v = x;

	if (v < 0):
	{
		v = 0;
	}

	return v;
}
```

```flluf
void trim(mut string s):
{
	s = "ok";
}
```

## Calling a function

Call a function by name with its arguments:

```flluf
int result = add(2, 3);   // 5
```

Arguments are converted to match the declared parameters where numeric
conversion is allowed (see
[Operators — Where `int -> float` conversion is allowed](operators.md#where-int---float-conversion-is-allowed)).

## `Main`

Every program needs one `Main` function with no parameters. It is the entry
point executed when the program starts:

```flluf
void Main():
{
	Log("Hello, world!");
}
```

`Main` may be declared with any return type; calling `Exit` or ending normally
both work.

## Overloading

Multiple functions may share a name as long as they have different parameter
lists. The compiler picks the overload whose parameter types match the call:

```flluf
int test():
{
	return 1;
}

int test(string s):
{
	return 2;
}

int test(int n):
{
	return 3;
}

void Main():
{
	Log(test());        // 1
	Log(test("x"));     // 2
	Log(test(7));       // 3
}
```

Each overload is a distinct function; the compiler resolves which one a call
refers to based on the argument types.

## Recursion

Functions may call themselves:

```flluf
int fact(int n):
{
	if (n <= 1):
	{
		return 1;
	}

	return n * fact(n - 1);
}
```
