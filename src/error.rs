use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

impl Pos {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

pub fn render(file: &Path, pos: Pos, msg: &str, lines: &[String]) -> String {
    let mut out = format!("{msg}\n  --> {}:{}:{}\n", file.display(), pos.line, pos.col);

    if let Some(src) = lines.get(pos.line.saturating_sub(1)) {
        let bar = "     |";

        out.push_str(&format!(
            "{bar}\n{:>4} | {src}\n{bar} {}^\n",
            pos.line,
            " ".repeat(pos.col.saturating_sub(1))
        ));
    }

    out
}
