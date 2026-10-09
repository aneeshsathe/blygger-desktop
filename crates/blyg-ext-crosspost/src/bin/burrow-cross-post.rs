//! cross-post as a standalone BXP executable (the tests run this; the app
//! runs the same code as `blygger +ext cross-post`).

fn main() -> std::process::ExitCode {
    blyg_ext_crosspost::run_stdio()
}
