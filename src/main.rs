mod ast;
mod codegen;
mod lexer;
mod parser;
mod token;

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: flluf-lang <input.fll> [-o output]");
        std::process::exit(1);
    }

    let input_path = PathBuf::from(&args[1]);

    let output_path = if let Some(pos) = args.iter().position(|a| a == "-o") {
        args.get(pos + 1).cloned().map(PathBuf::from)
    } else {
        Some(input_path.with_extension("o"))
    };

    let source = std::fs::read_to_string(&input_path).unwrap_or_else(|e| {
        eprintln!("Error reading file: {e}");
        std::process::exit(1);
    });

    let tokens: Vec<token::Token> = lexer::Lexer::new(&source).collect();
    let mut parser = parser::Parser::new(tokens);
    let program = parser.parse_program();

    let obj_data = codegen::compile(&program);

    let obj_path = output_path.unwrap_or_else(|| input_path.with_extension("o"));

    std::fs::write(&obj_path, &obj_data).unwrap_or_else(|e| {
        eprintln!("Error writing object file: {e}");
        std::process::exit(1);
    });

    let exe_path = obj_path.with_extension("");
    let runtime_path = obj_path.parent().unwrap().join("runtime.c");

    std::fs::write(&runtime_path, RUNTIME_C).unwrap_or_else(|e| {
        eprintln!("Error writing runtime: {e}");
        std::process::exit(1);
    });

    let cc = if cfg!(target_os = "linux") {
        "gcc"
    } else {
        "cc"
    };

    let status = Command::new(cc)
        .arg("-o")
        .arg(&exe_path)
        .arg(&obj_path)
        .arg(&runtime_path)
        .status()
        .unwrap_or_else(|e| {
            eprintln!("Error linking: {e}");
            std::process::exit(1);
        });

    if !status.success() {
        eprintln!("Linker failed");
        std::process::exit(1);
    }

    println!("Compiled: {}", exe_path.display());
}

const RUNTIME_C: &str = r##"#include <stdio.h>
#include <stdlib.h>

long long __flluf_log_i64(long long val) {
    printf("%lld\n", val);
    return val;
}

double __flluf_log_f64(double val) {
    printf("%f\n", val);
    return val;
}

const char* __flluf_log_ptr(const char* val) {
    printf("%s\n", val);
    return val;
}
"##;
