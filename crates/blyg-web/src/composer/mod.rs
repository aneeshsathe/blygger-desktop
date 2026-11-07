//! --- web-writing --- The native composer's pure parts, compiled
//! unchanged: the `@` mention list (`crates/blyg-app/src/composer/
//! mention.rs`: when typing `@` opens it, which blygs and contacts it
//! offers, how they rank, what a pick inserts) and the code-fence rules it
//! calls (`composer/text.rs`).

#[path = "../../../blyg-app/src/composer/text.rs"]
#[allow(dead_code)]
pub mod text;

#[path = "../../../blyg-app/src/composer/mention.rs"]
#[allow(dead_code)]
pub mod mention;
