//! markdown-notes as a standalone BXP executable (the tests run this; the
//! app runs the same code as `blygger +ext markdown-notes`).

fn main() -> std::process::ExitCode {
    blyg_ext_notes::run_stdio()
}
