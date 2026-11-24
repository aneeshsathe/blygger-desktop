// The wasm module's `env` imports. blyg-core links SQLite (its store) and
// ring (its HTTP client's TLS). Their C halves aren't built for wasm32 here
// (serve.sh links with --allow-undefined), and blyg-web never opens the
// store or the network. A few C-ABI exports that nobody calls still name
// these functions, so they need an import. If one of them is ever called,
// it throws instead of failing quietly. serve.sh checks that this list
// covers every `env` import.
const missing = (name) => () => {
  throw new Error(`${name} is native code that isn't in Burrow's web build`);
};
export const ring_core_0_17_14__bn_from_montgomery_in_place = missing('ring bn_from_montgomery_in_place');
export const ring_core_0_17_14__limbs_mul_add_limb = missing('ring limbs_mul_add_limb');
export const sqlite3_vfs_find = missing('sqlite3_vfs_find');
export const sqlite3_vfs_register = missing('sqlite3_vfs_register');
export const sqlite3_vfs_unregister = missing('sqlite3_vfs_unregister');
