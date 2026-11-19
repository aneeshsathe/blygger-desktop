//! `SwitchBackend` (crates/blyg-app/src/connection.rs) is the backend the
//! real app runs on: it hands every `Backend` call to the active blyg's
//! backend. Most `Backend` methods have default bodies (so a new method
//! doesn't break every implementor), which means a method `SwitchBackend`
//! doesn't forward still compiles and silently answers with the default.
//! That shipped twice in 0.18 (the shelf methods; `save_typing`, which put
//! typing back on the UI thread, and four more), each time passing every
//! test that used the fake backend directly.
//!
//! This test reads both files and fails, naming the methods, when a
//! `Backend` method has no forwarding body in `SwitchBackend`.

use std::collections::BTreeSet;
use std::path::Path;

use quote::ToTokens;

fn parse(path: &Path) -> syn::File {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    syn::parse_file(&text).unwrap_or_else(|e| panic!("{path:?}: {e}"))
}

/// Every method the `Backend` trait declares.
fn trait_methods(file: &syn::File) -> BTreeSet<String> {
    file.items
        .iter()
        .find_map(|item| match item {
            syn::Item::Trait(t) if t.ident == "Backend" => Some(t),
            _ => None,
        })
        .expect("trait Backend in backend.rs")
        .items
        .iter()
        .filter_map(|i| match i {
            syn::TraitItem::Fn(f) => Some(f.sig.ident.to_string()),
            _ => None,
        })
        .collect()
}

/// `SwitchBackend`'s methods whose bodies call the method of the same name
/// (on the active backend): `fn items(&self) { self.cur().items() }`.
fn forwarded(file: &syn::File) -> BTreeSet<String> {
    let imp = file
        .items
        .iter()
        .find_map(|item| match item {
            syn::Item::Impl(i)
                if i.trait_.as_ref().is_some_and(|(_, p, _)| {
                    p.segments.last().is_some_and(|s| s.ident == "Backend")
                }) && i.self_ty.to_token_stream().to_string() == "SwitchBackend" =>
            {
                Some(i)
            }
            _ => None,
        })
        .expect("impl Backend for SwitchBackend in connection.rs");
    imp.items
        .iter()
        .filter_map(|i| match i {
            syn::ImplItem::Fn(f) => {
                let name = f.sig.ident.to_string();
                let body = f.block.to_token_stream().to_string();
                body.contains(&format!(". {name} (")).then_some(name)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn switch_backend_forwards_every_backend_method() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let wanted = trait_methods(&parse(&crates.join("blyg-core/src/backend.rs")));
    let have = forwarded(&parse(&crates.join("blyg-app/src/connection.rs")));
    assert!(
        wanted.len() > 100,
        "found only {} Backend methods",
        wanted.len()
    );
    let missing: Vec<_> = wanted.difference(&have).collect();
    assert!(
        missing.is_empty(),
        "SwitchBackend (crates/blyg-app/src/connection.rs) doesn't forward {missing:?}: \
         the app would answer with the trait's default instead of the blyg's. Add \
         `fn <name>(..) {{ self.cur().<name>(..) }}` for each."
    );
}
