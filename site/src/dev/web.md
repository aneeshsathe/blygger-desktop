# blyg-web (preview)

`crates/blyg-web` is Burrow in a browser and on a phone: Burrow's own Rust
(the omnibar and list rules, the counter, the Markdown preview and the
reading view-model) compiled to WebAssembly, drawn by a small page of plain
ES modules. At phone widths it has bottom tabs (Write · Read · Shelf), a ＋
for quick capture and a paged bar above the keyboard; it installs as an
offline web app.

It runs on invented sample posts kept in the browser (IndexedDB) and sends
nothing anywhere. Serving it from a blyg at `{mount}/burrow`, beside
`/studio`, is proposed upstream in blygger-studio#139.

```sh
./crates/blyg-web/serve.sh               # build, then serve on http://127.0.0.1:8787/
./crates/blyg-web/serve.sh --build-only  # just build crates/blyg-web/dist/
```

The build is reproducible (byte-identical `dist/` from two clean builds)
with a pinned toolchain, `Cargo.lock`, wasm-bindgen and wasm-opt. Size
(wasm, `wasm-release` profile): about 1.1 MB gzip. The crate's
`README.md` has the details: what runs in Rust, the size breakdown, and how
to try it on a phone.
