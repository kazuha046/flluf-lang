use std::path::Path;
use std::process::{Command, Output};

const POSITIVE: &[&str] = &[
    "base",
    "comparison",
    "count-down",
    "else-if-chain",
    "float-test",
    "nested-if",
];

const NEGATIVE: &[&str] = &[
    "error",
    "no-main",
    "return-error",
    "return-void",
    "string-arithmetic",
    "top-level-stmt",
];

fn flluf() -> Command {
    Command::new(env!("CARGO_BIN_EXE_flluf"))
}

fn run(rel: &str) -> Output {
    flluf()
        .arg("run")
        .arg(rel)
        .output()
        .unwrap_or_else(|e| panic!("failed to run flluf on {rel}: {e}"))
}

fn stderr_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn stdout_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn positive_tests_compile_and_match_expected() {
    for name in POSITIVE {
        let out = run(&format!("tests/flluf/{name}.fll"));

        assert!(
            out.status.success(),
            "{name}.fll failed to compile/run:\n{}",
            stderr_of(&out)
        );

        let expected = std::fs::read_to_string(Path::new("tests/flluf/expected").join(format!("{name}.out")))
            .unwrap_or_else(|e| panic!("missing expected output for {name}: {e}"));

        assert_eq!(stdout_of(&out), expected, "{name}.fll stdout mismatch");
    }
}

#[test]
fn negative_tests_fail_to_compile() {
    for name in NEGATIVE {
        let out = run(&format!("tests/flluf/{name}.fll"));

        assert!(
            !out.status.success(),
            "{name}.fll unexpectedly compiled and ran"
        );
    }
}

#[test]
fn modules_project_matches_expected() {
    let out = run("tests/modules");

    assert!(
        out.status.success(),
        "tests/modules failed to compile/run:\n{}",
        stderr_of(&out)
    );

    let expected = std::fs::read_to_string("tests/modules/expected.out")
        .expect("missing expected output for modules");

    assert_eq!(stdout_of(&out), expected, "modules stdout mismatch");
}
