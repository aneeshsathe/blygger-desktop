//! Writes the two `Backend` impls in src/connection.rs from the `Backend`
//! trait, read from blyg-core's backend.rs on every build. There is no list
//! to keep.
//!
//! - `SwitchBackend` is the backend the real app runs on; it hands every
//!   call to the active blyg's backend: `fn m(&self, a..) -> R {
//!   self.cur().m(a..) }`, defaulted methods included. A hand-written
//!   forward that was missing still compiled and silently answered with the
//!   default: that shipped twice in 0.18 (081edf3, 2616a3f).
//! - `Disconnected` (no blyg yet) answers every required method from its
//!   return type ([`not_connected_answer`]): a `Result` is
//!   `not_connected()`, an `Option` is `None`, a `bool` is `false`, a
//!   number or collection is empty. A defaulted method keeps the trait's
//!   default. A return type with no rule fails the build, naming the fix.
//!
//! A method a type must answer differently is written in its inherent
//! `impl SwitchBackend` / `impl Disconnected` under the trait method's
//! name; the generated impl calls that instead (`set_event_sink` for the
//! switch; the answers `Disconnected` gives that the rules above don't).

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
    let connection = parse(&connection);

    let switch = trait_impl(
        backend_trait,
        "SwitchBackend",
        &hand_written(&connection, "SwitchBackend"),
        |f, args| {
            let name = &f.sig.ident;
            Some(Body {
                code: quote! { self.cur().#name(#(#args),*) },
                uses_args: true,
            })
        },
    );
    std::fs::write(out.join("switch_forwarding.rs"), switch.to_string())
        .expect("write switch_forwarding.rs");

    let disconnected = trait_impl(
        backend_trait,
        "Disconnected",
        &hand_written(&connection, "Disconnected"),
        |f, _| {
            f.default.is_none().then(|| Body {
                code: not_connected_answer(&f.sig),
                uses_args: false,
            })
        },
    );
    std::fs::write(out.join("disconnected.rs"), disconnected.to_string())
        .expect("write disconnected.rs");
}

/// A generated method body; `uses_args: false` names its arguments `_`.
struct Body {
    code: TokenStream,
    uses_args: bool,
}

/// `impl Backend for <ty>`: each trait method calls `<ty>::m` when `own`
/// lists it, else gets `body`'s answer; `None` leaves the trait default.
fn trait_impl(
    backend_trait: &syn::ItemTrait,
    ty: &str,
    own: &[String],
    body: impl Fn(&syn::TraitItemFn, &[Ident]) -> Option<Body>,
) -> TokenStream {
    let ty = Ident::new(ty, Span::call_site());
    let mut methods = TokenStream::new();
    for item in &backend_trait.items {
        let syn::TraitItem::Fn(f) = item else {
            continue;
        };
        let name = &f.sig.ident;
        let mut args = Vec::new();
        let mut sig = f.sig.clone();
        for (i, input) in sig.inputs.iter_mut().enumerate() {
            if let syn::FnArg::Typed(t) = input {
                let a = Ident::new(&format!("a{i}"), Span::call_site());
                *t.pat = syn::parse_quote!(#a);
                args.push(a);
            }
        }
        let body = if own.contains(&name.to_string()) {
            quote! { #ty::#name(self, #(#args),*) }
        } else {
            let Some(b) = body(f, &args) else {
                continue;
            };
            if !b.uses_args {
                for input in sig.inputs.iter_mut() {
                    if let syn::FnArg::Typed(t) = input {
                        *t.pat = syn::parse_quote!(_);
                    }
                }
            }
            b.code
        };
        let cfgs = f.attrs.iter().filter(|a| a.path().is_ident("cfg"));
        // Paths in backend.rs say `crate::`; here that crate is blyg_core.
        let sig = crate_is_blyg_core(sig.to_token_stream());
        methods.extend(quote! {
            #(#cfgs)*
            #sig { #body }
        });
    }
    quote! { impl Backend for #ty { #methods } }
}

/// `Disconnected`'s answer to a required method, from its return type.
/// Panics (failing the build) on a type with no rule, naming the fix.
fn not_connected_answer(sig: &syn::Signature) -> TokenStream {
    let syn::ReturnType::Type(_, ty) = &sig.output else {
        return quote! {};
    };
    let last = match &**ty {
        syn::Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        syn::Type::Tuple(t) if t.elems.is_empty() => return quote! {},
        _ => None,
    };
    match last.as_deref() {
        Some("Result") => quote! { not_connected() },
        Some("Option") => quote! { None },
        Some("bool") => quote! { false },
        Some(
            "usize" | "u8" | "u16" | "u32" | "u64" | "isize" | "i8" | "i16" | "i32" | "i64" | "Vec"
            | "HashMap" | "HashSet" | "BTreeMap" | "BTreeSet" | "String",
        ) => quote! { Default::default() },
        _ => panic!(
            "\n\nbuild_forwarding.rs: `Disconnected` has no answer for the required \
             `Backend::{name}` (returns `{ty}`).\n\
             Write it by hand in `impl Disconnected` in crates/blyg-app/src/connection.rs:\n\
             \n    fn {name}(&self, ..) -> {ty} {{ .. }}\n\n\
             (or give the trait method a default, if every backend may answer that way).\n",
            name = sig.ident,
            ty = ty
                .to_token_stream()
                .to_string()
                .replace(" :: ", "::")
                .replace(" <", "<")
                .replace("< ", "<")
                .replace(" >", ">"),
        ),
    }
}

fn parse(path: &Path) -> syn::File {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    syn::parse_file(&text).unwrap_or_else(|e| panic!("{path:?}: {e}"))
}

/// The methods `ty`'s inherent impls write themselves.
fn hand_written(file: &syn::File, ty: &str) -> Vec<String> {
    file.items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Impl(i)
                if i.trait_.is_none() && i.self_ty.to_token_stream().to_string() == ty =>
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
