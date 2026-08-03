# Command Line Interface

`flluf` has three commands: `build`, `run`, and `new`.

```
Usage: flluf <COMMAND>

Commands:
  build
  run
  new
```

## `flluf run <FILE>`

Compiles the project at `<FILE>` and runs the resulting executable. `<FILE>` may
be a single `.fll` file or a project directory with `init.toml`:

```sh
flluf run main.fll
flluf run .
```

The program's output is followed by an exit line printed by the driver:

```text
Hello, world!
program exited with code 0
```

If compilation fails, the errors are printed instead and nothing is run.

## `flluf build <FILE> [-o <OUTPUT>]`

Compiles without running and writes the executable to disk. By default the
binary is placed in a `target/` directory next to the input; use `-o` to choose
an explicit output path:

```sh
flluf build main.fll -o hello
./hello
```

## `flluf new <NAME>`

Scaffolds a new project in a directory named `<NAME>` under the current
directory:

```sh
flluf new hello
cd hello
flluf run .
```

Fails if the directory already exists.

## Exit status

- `0` — success
- non-zero — the program ran with that exit code, or a build error occurred
