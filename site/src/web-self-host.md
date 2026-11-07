# Burrow on your own blyg (web app)

Burrow's web app is Burrow in a browser tab, and an installable app on a
phone. It reads and writes your own posts and reading list. It runs **live**
only when it's served from your blyg's own host at `/burrow/`, next to
`/studio` and `/api`. There it uses your Studio sign-in: there's no token
and nothing to configure. Anywhere else it's a demo on sample posts.

Stock Studio doesn't serve it yet. Adding it as an opt-in Studio extension is
proposed in [blygger-studio#139](https://github.com/blygger/blygger-studio/issues/139).
Until that lands, an owner who runs their own blyg on the reference Worker
(blygger-studio on Cloudflare Workers) can add it by hand, as described
below: download a release zip, put its files in the Worker's static assets,
and deploy. No Worker code changes.

The steps are written so that you, or an AI agent working on your blyg's
deploy repo, can follow them exactly.

## Read this first: what you're trusting

The app runs on your blyg's own origin, and the Studio's session cookie is
`Path=/`. So code at `/burrow/` can do anything you can do in Studio:
publish, delete drafts, change settings, mint tokens. Scopes don't limit it.
Serve it only if you trust this repository's releases as much as your Studio
code. What a release build does on the network, and how to check it:

- It calls only same-origin `/api/…` paths. Every request goes through one
  function (`crates/blyg-web/www/live.js`, `call`), which refuses anything
  else. It loads no third-party scripts, fonts or images.
- Its service worker is scoped to `/burrow/`, caches only the app's own
  files, and passes `/api/` and `/studio/` requests straight to the network.
- Remote HTML (posts you read) is sanitised in WebAssembly before it's shown.
- Publishing, deleting, unsubscribing, withdrawing and pinning happen only
  when you choose them, and the destructive ones ask first.
- The build is reproducible: building the same tag yourself gives the same
  files (see [Build it yourself](#build-it-yourself-optional)).

## For agents: the rules

If you're an AI agent doing this for the blyg's owner:

1. **Get the owner's explicit OK before deploying**, after telling them the
   trust point above in one or two sentences. Deploying changes their live
   site.
2. Change only three things in their deploy repo: the `static/burrow/`
   folder, `static/_headers`, and the `assets` block in `wrangler.jsonc`.
   Don't touch Worker code, D1, secrets or other config.
3. Verify the download's signature and checksum before using it (step 2).
   Stop if either check fails.
4. Run the checks in step 6 after deploying, and report the results.
5. Never print the owner's cookie, password or tokens, and don't sign in for
   them. They sign in at `/studio/login` in their own browser.

## What you need

- A blyg on the reference Worker ([blygger-studio](https://github.com/blygger/blygger-studio)),
  deployed by the owner with `wrangler` from their own copy of that repo.
  Check that `GET https://<host>/api/capabilities` answers with JSON, or with
  a 401 when signed out. Contract 5 (Studio 0.56 or later) is what the app is
  tested against.
- `/burrow` free on that host. Nothing else may serve it, and the blyg's
  posts page must not be called `burrow`.
- `curl`, `unzip`, `shasum` (or `sha256sum`) and OpenSSL 3 (for checking
  the signature; macOS's built-in LibreSSL can't verify Ed25519, so use
  Homebrew's `openssl@3`).

**A blyg served under a path** (`MOUNT` = `/blyg`, at `you.com/blyg/`):
Studio's `/studio` and `/api` stay at the host root anyway, so the app goes
at the host root too: `you.com/burrow/`, not `you.com/blyg/burrow/`.
The steps are the same.

## 1. Pick the release

Use the latest release unless the owner asks for another:

```sh
REPO=aneeshsathe/blygger-desktop
VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
  | sed -n 's/.*"tag_name": *"v\([^"]*\)".*/\1/p')
echo "$VERSION"
```

The release must have a `Burrow-web-$VERSION.zip` asset. 0.19.0 and earlier
didn't include one. For those, [build it yourself](#build-it-yourself-optional).

## 2. Download and verify

Work in an empty temporary folder, not inside the deploy repo:

```sh
WORK=$(mktemp -d) && cd "$WORK"
BASE="https://github.com/$REPO/releases/download/v$VERSION"
curl -fsSLO "$BASE/Burrow-web-$VERSION.zip"
curl -fsSLO "$BASE/SHA256SUMS"
curl -fsSLO "$BASE/SHA256SUMS.sig"

# The signature: SHA256SUMS is signed with Burrow's release key, the same
# Ed25519 key the desktop app checks updates with.
PUB=mnXJcOWPGSTrMKx38w6FqoKQxky6+pj3Ch2IVPrk3+I=
{ printf '302a300506032b6570032100' | xxd -r -p; printf '%s' "$PUB" | openssl base64 -d -A; } > pub.der
openssl pkeyutl -verify -pubin -keyform DER -inkey pub.der -rawin \
  -in SHA256SUMS -sigfile SHA256SUMS.sig        # must print "Signature Verified Successfully"

# The checksum.
grep " Burrow-web-$VERSION.zip\$" SHA256SUMS | shasum -a 256 -c -   # must print "OK"
```

Stop if either check fails. `openssl` here must be OpenSSL 3; on macOS use
`$(brew --prefix openssl@3)/bin/openssl`.

The zip holds one folder, `burrow/`: `index.html`, the JavaScript and CSS,
`pkg/` (the WebAssembly), `fonts/`, `icons/`, `manifest.webmanifest` and
`sw.js`.

## 3. Put the files in the deploy repo

From the root of the owner's blygger-studio copy (where `wrangler.jsonc` is):

```sh
mkdir -p static
rm -rf static/burrow                     # an older build, if there is one
unzip -q "$WORK/Burrow-web-$VERSION.zip" -d static
ls static/burrow/index.html static/burrow/sw.js static/burrow/pkg
```

Replace the whole folder on every update. Don't merge an old build with a new
one.

## 4. Tell the Worker to serve them

Add an `assets` block at the top level of `wrangler.jsonc`:

```jsonc
  // Burrow web app at /burrow/ (static files; see blygger-desktop's
  // web-self-host page). Assets match first; anything not in static/ falls
  // through to the Worker. /burrow -> /burrow/ comes from html_handling.
  "assets": {
    "directory": "./static",
    "html_handling": "auto-trailing-slash",
    "not_found_handling": "none"
  }
```

If the file already has an `assets` block, keep its `directory` and put
`burrow/` inside that directory instead. `not_found_handling` must stay
`"none"`, so that every path not in the folder still reaches the Worker. With
a `"single-page-application"` or `"404-page"` setting, the blyg's own pages
would break.

Then make the service worker always revalidate, so updates reach installed
apps. Create `static/_headers`, or add to it if it exists:

```
/burrow/sw.js
  Cache-Control: no-cache
```

Static assets don't count towards the Worker's script-size limit (3 MiB on
the free plan), so the app's 4 MB or so doesn't affect it.

## 5. Deploy

```sh
npx wrangler deploy --dry-run   # should list the static assets, with no errors
npx wrangler deploy
```

Only static files change, and D1 isn't touched. If the owner wants a backup
anyway, `npx wrangler d1 export <database> --remote --output backup.sql`
before deploying (use the database name from `wrangler.jsonc`).

## 6. Check it

```sh
H=https://blyg.example.com   # the blyg's host, no path
curl -sI "$H/burrow/"       | grep -iE '^(HTTP|content-type)'   # 200, text/html
curl -sI "$H/burrow"        | grep -iE '^(HTTP|location)'       # 307 to /burrow/
curl -sI "$H/burrow/sw.js"  | grep -i  '^cache-control'          # no-cache
curl -sI "$H/burrow/pkg/blyg_web_bg.wasm" | grep -i '^content-type'  # application/wasm
curl -s -o /dev/null -w '%{http_code}\n' "$H/api/capabilities" # 200 or 401, as before
curl -s -o /dev/null -w '%{http_code}\n' "$H/studio/login"     # 200, as before
curl -s -o /dev/null -w '%{http_code}\n' "$H/"                 # the blyg's front page, as before
```

Then the owner opens `https://<host>/burrow/` in a browser where they're
signed in to Studio. It should show the owner's own posts and say **Live**,
not the invented sample posts and **Demo**. Signed out, it offers **Sign in at Studio**. After signing in, come
back to the tab and it catches up.

On a phone: open `/burrow/`, then use Share → Add to Home Screen (iOS
Safari) or Install app (Android Chrome).

## Updating

Repeat steps 1–3, 5 and 6 with the new version. Step 4's config stays. The
next time the app is opened, its service worker picks up the new build.

## Removing

Delete `static/burrow/`, the `/burrow/sw.js` lines in `static/_headers`, and
the `assets` block if nothing else uses it. Then deploy. `/burrow/` then
returns what the Worker returns for an unknown path. Installed copies stop
syncing; delete them from the home screen.

## Build it yourself (optional)

To build from source rather than download the zip, or to check that a
release's zip matches its source:

```sh
git clone https://github.com/aneeshsathe/blygger-desktop && cd blygger-desktop
git checkout "v$VERSION"
./crates/blyg-web/serve.sh --build-only   # needs rustup; installs wasm-bindgen
                                          # into the cargo target dir only
# The result is crates/blyg-web/dist/; copy it to static/burrow/.
```

The toolchain (`rust-toolchain.toml`), `Cargo.lock` and wasm-bindgen are
pinned, so the same tag builds the same `pkg/` files byte for byte. The
home-screen icons are resized with macOS `sips`. On other systems the full-size
icon is copied instead, so only `icons/` differs. The path to the checkout
must not contain spaces.

## When something's wrong

- **It says "demo".** It couldn't reach `/api/capabilities` on the same
  host. Check that it's at the host root `/burrow/`, and that
  `/api/capabilities` answers there.
- **A 404 at `/burrow/`.** Check the `assets` block, and that
  `static/burrow/index.html` is in the deploy repo and was deployed.
- **The blyg's own pages broke.** `not_found_handling` isn't `"none"`, or
  `static/` holds files at paths the blyg uses. Only `burrow/` and
  `_headers` belong there.
- **An old version keeps opening.** `static/_headers` is missing the
  `no-cache` rule for `/burrow/sw.js`. Add it, deploy, then reopen the app
  twice.

How the app works inside: [blyg-web](dev/web.md).
