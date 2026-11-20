---
name: client-security
description: Security review for a Blygger client before merge or deploy. Use when a change renders remote content, adds a network call, touches tokens or the owner session, or adds an innerHTML sink.
---

# Client security checklist

A Blygger client usually holds owner rights to a blyg and renders other
people's HTML. Treat every remote byte as hostile.

## Remote content

- **Sanitize remote HTML** through one allowlist sanitizer (e.g. ammonia), and
  only that path. Never assign remote HTML to `innerHTML` unsanitized; prefer
  text nodes. New `innerHTML` sinks need a reason in the PR.
- **MathML (spec §14):** a `math` element must not pass through unexamined:
  keep it under a MathML Core allowlist, redraw it from its `application/x-tex`
  annotation, replace it with that annotation as text (built from the one x-tex
  string, nothing else copied), or drop it.
- `svg`, `script`, `style`, `iframe`, `object`, `embed`, `template`, `noscript`
  are dropped with their contents unless there's a reviewed reason.
- Link schemes: http(s) and `mailto:` only; refuse `javascript:`, `data:`,
  `file:`, `blob:` and custom schemes.

## Network and identity

- **Same-origin only** for a web client with owner rights: calls go to its own
  blyg's `/api/`; no third-party origins (fonts, CDNs, analytics, OCR models).
  A feature that needs an external fetch needs a server-side route first, or it
  waits.
- **Never print or log tokens**, including in tests and CI output. Show a newly
  minted token once; don't store it client-side.
- **Confirm destructive actions** (withdraw, delete, revoke, mark all read).
- Embedded browsers/web views get no IPC bridge to the app unless reviewed.

## Repos

- Public repos carry no personal data: no names, emails, real blyg URLs,
  home-directory paths or account ids. Use `blyg.example.com` and invented text.

## In Burrow

- Reader HTML: `crates/blyg-app/src/studio/sanitize.rs` (`sanitize_at`, ammonia);
  rendered Markdown: `crates/blyg-render/src/sanitize.rs` (mirrors the studio's
  sanitizer). Both drop `math` today, which §14 allows.
- The browser pane has no IPC handler or init script; keep it that way.
- Secrets live in the macOS Keychain (Credential Manager on Windows), config in
  `~/.config/blygger/config`; neither is ever in the repo.
