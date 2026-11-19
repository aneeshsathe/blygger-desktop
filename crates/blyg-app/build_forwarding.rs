//! Writes `SwitchBackend`'s forwarding methods from the `Backend` trait.
//!
//! `SwitchBackend` (src/connection.rs) is the backend the real app runs on;
//! it hands every call to the active blyg's backend. Most `Backend` methods
//! have default bodies, so a hand-written forward that was missing still
//! compiled and silently answered with the default: that shipped twice in
//! 0.18. Now the whole `impl Backend for SwitchBackend` is written here, read
//! from blyg-core's backend.rs on every build: each method becomes
//! `fn m(&self, a..) -> R { self.cur().m(a..) }`. There is no list to keep.
//! A method `SwitchBackend` must do differently is written in its inherent
//! `impl SwitchBackend` under the trait method's name; the trait impl calls
//! that instead (today only `set_event_sink`).

use std::path::Path;

use proc_macro2::{Ident, Span, TokenStream, TokenTree};
use quote::{ToTokens, quote};

pub fn generate(manifest: &Path, out: &Path) {
    let backend = manifest.join("../blyg-core/src/backend.rs");
    let connection = manifest.join("src/connection.rs");
    println!("cargo:rerun-if-changed={}", backend.display());
    println!("cargo:rerun-if-changed={}", connection.display());

    let trait_file = parse(&backend);
    let backend_trait = trait_file
        .items
        .iter()
        .find_map(|i| match i {
            syn::Item::Trait(t) if t.ident == "Backend" => Some(t),
            _ => None,
        })
        .expect("trait Backend in blyg-core/src/backend.rs");
    let written = hand_written(&parse(&connection));

    let mut methods = TokenStream::new();
    for item in &backend_trait.items {
        let syn::TraitItem::Fn(f) = item else {
            continue;
        };
        let own = written.contains(&f.sig.ident.to_string());
        let mut sig = f.sig.clone();
        let mut args = Vec::new();
        for (i, input) in sig.inputs.iter_mut().enumerate() {
            if let syn::FnArg::Typed(t) = input {
                let name = Ident::new(&format!("a{i}"), Span::call_site());
                *t.pat = syn::parse_quote!(#name);
                args.push(name);
            }
        }
        let name = &sig.ident;
        let cfgs = f.attrs.iter().filter(|a| a.path().is_ident("cfg"));
        // Paths in backend.rs say `crate::`; here that crate is blyg_core.
        let sig = crate_is_blyg_core(sig.to_token_stream());
        let body = if own {
            quote! { SwitchBackend::#name(self, #(#args),*) }
        } else {
            quote! { self.cur().#name(#(#args),*) }
        };
        methods.extend(quote! {
            #(#cfgs)*
            #sig { #body }
        });
    }
    let code = quote! { impl Backend for SwitchBackend { #methods } };
    std::fs::write(out.join("switch_forwarding.rs"), code.to_string())
        .expect("write switch_forwarding.rs");
}

fn parse(path: &Path) -> syn::File {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    syn::parse_file(&text).unwrap_or_else(|e| panic!("{path:?}: {e}"))
}

/// The methods `SwitchBackend`'s inherent impls write themselves.
fn hand_written(file: &syn::File) -> Vec<String> {
    file.items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Impl(i)
                if i.trait_.is_none()
                    && i.self_ty.to_token_stream().to_string() == "SwitchBackend" =>
            {
                Some(i)
            }
            _ => None,
        })
        .flat_map(|i| &i.items)
        .filter_map(|i| match i {
            syn::ImplItem::Fn(f) => Some(f.sig.ident.to_string()),
            _ => None,
        })
        .collect()
}

fn crate_is_blyg_core(tokens: TokenStream) -> TokenStream {
    tokens
        .into_iter()
        .map(|t| match t {
            TokenTree::Ident(i) if i == "crate" => {
                TokenTree::Ident(Ident::new("blyg_core", i.span()))
            }
            TokenTree::Group(g) => {
                let mut n = proc_macro2::Group::new(g.delimiter(), crate_is_blyg_core(g.stream()));
                n.set_span(g.span());
                TokenTree::Group(n)
            }
            other => other,
        })
        .collect()
}
