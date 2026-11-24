// The keys the page binds, in one table: the key help under the window, the
// HUD that shows a key as it's pressed, tooltips and the Blyg list all read
// their spelling here. Browsers keep ⌘N, ⌘T and ⌘L for themselves, so the
// page binds ⌃ (it also takes ⌥, and ⌘ where the browser lets it go): every
// hint says ⌃. Hints from the Rust are spelled the same way
// (src/keymap.rs `browser_hint`); `ux_tests.rs` fails on a ⌘ in the page.

export const KEYS = {
  posts: '⌃1', reading: '⌃2', blyg: '⌃3',
  newPost: '⌃N', search: '⌃L', publish: '⌃⏎', kind: '⌃T', preview: '⌃E', draft: '⌃D',
  del: '⌃⇧⌫', discard: '⌃⌥Z', link: '⌃K', quote: '⌃⇧K', saved: '⌃B', save: '⌃⌥B',
  open: '⌃O', versions: '⌃⇧Y', markAll: '⌃⌥A', filter: '⌃⌥F',
};

/** A chord's keys, one per cap (`esc` and arrows stay whole). */
export const capsOf = (chord) => (/^[a-z]+$/.test(chord) ? [chord] : [...chord]);
/** A chord as `<kbd>`s. */
export const kbd = (chord) => capsOf(chord).map((k) => `<kbd>${k}</kbd>`).join('');

/** The key help under the desk's window, in the order a writer meets them. */
export const HELP = [
  ['⏎', 'open or create'], ['↑↓', 'move'], ['esc', 'back to search'],
  [KEYS.newPost, 'new post'], [KEYS.search, 'search'], [KEYS.publish, 'publish'],
  [KEYS.kind, 'fragment ⇄ thread'], [KEYS.preview, 'preview'], [KEYS.draft, 'make draft'],
  [KEYS.del, 'delete draft'], [KEYS.discard, 'discard changes'],
  [KEYS.link, 'link'], [KEYS.quote, 'quote'],
  [KEYS.saved, 'Saved'], [KEYS.save, 'save or unsave'], [KEYS.versions, 'versions'],
  [KEYS.posts, 'Posts'], [KEYS.reading, 'Reading'], [KEYS.blyg, 'Blyg'],
  [KEYS.markAll, 'mark all read (Reading)'], [KEYS.filter, 'next filter (Posts)'],
];
export const helpHtml = () => HELP.map(([k, what]) => `${kbd(k)} ${what}`).join(' · ');
