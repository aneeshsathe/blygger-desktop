// What the flow pass measures, and the journey runner both UIs share
// (scripts/browser-web-flow-pass.mjs). In the page (an init script, so it
// is there before the app's first byte):
//   - layout shifts (PerformanceObserver `layout-shift`; Chromium only) and
//     long tasks over 50 ms (`longtask`; Chromium), plus frames that came
//     more than 50 ms apart (every engine, WebKit's stand-in for both);
//   - every DOM change: the time of the last one (for "settled"), and
//     FLICKER: text removed and the same text added back after at least one
//     frame was painted between (seen gone, then back). Removed and re-added
//     in one frame is a REDRAW: invisible, but it is what loses a selection
//     or a caret;
//   - spinners (busy/progress/loading/skeleton elements) and empty states,
//     each frame: a spinner shown under 150 ms, or an empty state shown and
//     then replaced by content, is a flash.
// Outside the page, per step: requests started (and the same method + URL
// twice), time from the input until no request is pending and the DOM has
// been quiet for QUIET ms, whether focus is where the step expects, and
// whether the element the eye was on moved without the user scrolling.
import fs from 'node:fs';
import path from 'node:path';

const QUIET = 300;
const SETTLE_CAP = 15_000;

/** Runs in the page before anything else. */
function pageProbe() {
  if (window.__flow) return;
  const F = (window.__flow = {
    shifts: [], longTasks: [], jank: [], flicker: [], redraws: 0, spinners: [], empties: [],
    lastMutation: Date.now(), lastMutationWhat: '', frame: 0,
  });
  try {
    new PerformanceObserver((l) => {
      for (const e of l.getEntries()) {
        if (e.hadRecentInput) continue;
        const src = (e.sources || []).slice(0, 3).map((s) => {
          const n = s.node;
          if (!n || n.nodeType !== 1) return '?';
          return `${n.tagName.toLowerCase()}${n.id ? '#' + n.id : ''}${typeof n.className === 'string' && n.className ? '.' + n.className.trim().split(/\s+/).slice(0, 2).join('.') : ''}`;
        });
        F.shifts.push({ t: Date.now(), v: e.value, src });
      }
    }).observe({ type: 'layout-shift', buffered: true });
    F.clsSupported = true;
  } catch { F.clsSupported = false; }
  try {
    new PerformanceObserver((l) => { for (const e of l.getEntries()) F.longTasks.push({ t: Date.now(), d: Math.round(e.duration) }); })
      .observe({ type: 'longtask', buffered: true });
    F.longTaskSupported = true;
  } catch { F.longTaskSupported = false; }

  const removed = new Map(); // text key -> { t, frame }
  const keyOf = (n) => {
    if (n.nodeType !== 1 && n.nodeType !== 3) return null;
    const t = (n.textContent || '').replace(/\s+/g, ' ').trim();
    return t.length >= 24 ? t.slice(0, 160) : null;
  };
  const start = () => {
    new MutationObserver((recs) => {
      const now = Date.now();
      F.lastMutation = now;
      const tg = recs[recs.length - 1].target;
      F.lastMutationWhat = tg.nodeType === 1 ? `${tg.tagName.toLowerCase()}${tg.id ? '#' + tg.id : ''}.${String(tg.className).split(' ')[0]} ${recs[recs.length - 1].type} ${recs[recs.length - 1].attributeName || ''}` : 'text';
      for (const r of recs) {
        for (const n of r.removedNodes) {
          const k = keyOf(n);
          if (k) removed.set(k, { t: now, frame: F.frame });
        }
        for (const n of r.addedNodes) {
          const k = keyOf(n);
          if (!k) continue;
          const was = removed.get(k);
          if (!was) continue;
          removed.delete(k);
          if (was.frame < F.frame && now - was.t < 2000) F.flicker.push({ t: now, removedAt: was.t, gone: now - was.t, text: k.slice(0, 60) });
          else F.redraws++;
        }
      }
      if (removed.size > 4000) removed.clear();
    }).observe(document, { subtree: true, childList: true, characterData: true, attributes: true, attributeFilter: ['hidden', 'class', 'aria-busy', 'style'] });
  };
  if (document.documentElement) start(); else document.addEventListener('readystatechange', start, { once: true });

  const SPIN = '[aria-busy="true"],[role="progressbar"],.spinner,[class*="spinner" i],[class*="skeleton" i],[class*="loading" i]:not(.gone),[data-loading="true"]';
  const EMPTY = '.empty,.stream-empty,[class*="empty-state" i],[class*="EmptyState" i],[data-empty]';
  const shown = (el) => {
    const r = el.getBoundingClientRect();
    if (r.width < 2 || r.height < 2 || r.bottom < 0 || r.top > innerHeight) return false;
    if (el.closest('[hidden],[inert]')) return false;
    const cs = getComputedStyle(el);
    return cs.visibility !== 'hidden' && cs.display !== 'none' && +cs.opacity > 0.05;
  };
  const live = { spin: new Map(), empty: new Map() };
  let last = performance.now();
  const tick = (ts) => {
    F.frame++;
    const gap = ts - last;
    last = ts;
    if (gap > 50 && document.visibilityState === 'visible') F.jank.push({ t: Date.now(), gap: Math.round(gap) });
    if (document.body) {
      for (const [kind, sel, list] of [['spin', SPIN, F.spinners], ['empty', EMPTY, F.empties]]) {
        const now = new Set();
        for (const el of document.querySelectorAll(sel)) {
          if (kind === 'empty' && !(el.textContent || '').trim()) continue;
          if (shown(el)) now.add(el);
        }
        const m = live[kind];
        for (const el of now) if (!m.has(el)) m.set(el, Date.now());
        for (const [el, t0] of m) {
          if (!now.has(el)) {
            m.delete(el);
            list.push({ t: Date.now(), d: Date.now() - t0, what: `${el.tagName.toLowerCase()}${el.id ? '#' + el.id : ''}.${String(el.className).split(' ')[0]} "${(el.textContent || '').trim().slice(0, 40)}"`, gone: !el.isConnected });
          }
        }
      }
    }
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);

  F.since = (t) => ({
    shifts: F.shifts.filter((x) => x.t >= t),
    longTasks: F.longTasks.filter((x) => x.t >= t),
    jank: F.jank.filter((x) => x.t >= t),
    // Gone and back within the step (navigating away in one step and back
    // in the next is not flicker).
    flicker: F.flicker.filter((x) => x.removedAt >= t),
    spinners: F.spinners.filter((x) => x.t >= t),
    empties: F.empties.filter((x) => x.t >= t),
    redraws: F.redraws,
    lastMutation: F.lastMutation,
    clsSupported: F.clsSupported,
    longTaskSupported: F.longTaskSupported,
  });
}

export function metricsInit() { return pageProbe; }

/** Mark the element the eye is on: the first one whose top is in the
    middle band of the scroller (or the viewport), so it can be found again
    after the step. `sel` names the scroller. */
async function anchorBefore(page, sel) {
  return page.evaluate((sel) => {
    document.querySelectorAll('[data-flow-anchor]').forEach((e) => e.removeAttribute('data-flow-anchor'));
    const sc = sel ? document.querySelector(sel) : null;
    if (sel && !sc) return null;
    const box = sc ? sc.getBoundingClientRect() : { top: 0, bottom: innerHeight, left: 0, right: innerWidth };
    const y = box.top + Math.min(200, (box.bottom - box.top) / 3);
    const x = box.left + Math.min(160, (box.right - box.left) / 2);
    let el = document.elementFromPoint(x, y);
    // Something with text of its own, not the scroller itself.
    while (el && el !== sc && el.parentElement && (el.textContent || '').trim().length < 10) el = el.parentElement;
    if (!el || el === sc || el === document.body) return null;
    el.setAttribute('data-flow-anchor', '1');
    return { top: el.getBoundingClientRect().top, scroll: sc ? sc.scrollTop : scrollY, text: (el.textContent || '').trim().slice(0, 50) };
  }, sel ?? null);
}
async function anchorAfter(page, sel) {
  return page.evaluate((sel) => {
    const el = document.querySelector('[data-flow-anchor]');
    const sc = sel ? document.querySelector(sel) : null;
    return el ? { top: el.getBoundingClientRect().top, scroll: sc ? sc.scrollTop : scrollY, connected: true } : { connected: false };
  }, sel ?? null);
}

const describeFocus = (page) => page.evaluate(() => {
  const e = document.activeElement;
  if (!e || e === document.body) return 'body';
  return `${e.tagName.toLowerCase()}${e.id ? '#' + e.id : ''}${typeof e.className === 'string' && e.className ? '.' + e.className.trim().split(/\s+/).slice(0, 2).join('.') : ''}`;
}).catch(() => 'unknown');

/**
 * One journey: `fn(J)` walks it with J's helpers. Returns the record.
 * target 'burrow' | 'studio'; viewport 'desktop' | 'phone'.
 */
export async function runJourney({ browserType, browserName, target, journey, viewport, storageState, outDir, setup }, fn) {
  const phone = viewport === 'phone';
  const size = phone ? { width: 390, height: 844 } : { width: 1440, height: 900 };
  const browser = await browserType.launch();
  const context = await browser.newContext({
    viewport: size, isMobile: phone && browserName !== 'firefox', hasTouch: phone, deviceScaleFactor: phone ? 2 : 1, storageState,
  });
  await context.addInitScript(pageProbe);
  const page = await context.newPage();
  page.setDefaultTimeout(8000);
  const rec = { target, journey, viewport, browser: browserName, steps: [], notes: [], errors: [] };
  let cur = null; // the step collecting requests
  let pending = 0;
  const inflight = new Set();
  let lastReqEnd = Date.now();
  const reqOf = (r) => `${r.method()} ${(() => { const u = new URL(r.url()); return u.pathname + u.search; })()}`;
  page.on('request', (r) => {
    if (r.url().startsWith('data:')) return;
    pending++;
    inflight.add(r);
    if (cur) cur.requests.push(reqOf(r));
  });
  const done = (r) => { if (inflight.delete(r)) pending = Math.max(0, pending - 1); lastReqEnd = Date.now(); };
  page.on('requestfinished', done);
  page.on('requestfailed', done);
  page.on('pageerror', (e) => rec.errors.push(`page error: ${e.message.slice(0, 200)}`));
  page.on('console', (m) => { if (m.type() === 'error') rec.errors.push(`console: ${m.text().slice(0, 200)}`); });
  page.on('dialog', (d) => { rec.notes.push(`native dialog: ${d.message()}`); d.accept().catch(() => {}); });

  let unsettled = null;
  async function settle(t0) {
    unsettled = null;
    const until = t0 + SETTLE_CAP;
    for (;;) {
      await new Promise((r) => setTimeout(r, 50));
      const lm = await page.evaluate(() => window.__flow?.lastMutation ?? 0).catch(() => Date.now());
      const nowT = Date.now();
      if (pending === 0 && nowT - lm >= QUIET && nowT - lastReqEnd >= QUIET) return Math.max(0, Math.max(lm, lastReqEnd) - t0);
      if (nowT > until) {
        const what = await page.evaluate(() => window.__flow?.lastMutationWhat).catch(() => '');
        unsettled = { pending: [...inflight].slice(0, 4).map(reqOf), lastMutationAgo: nowT - lm, lastMutation: what };
        return SETTLE_CAP;
      }
    }
  }

  const J = {
    page, context, phone, sleep: (ms) => new Promise((r) => setTimeout(r, ms)),
    inputs: 0, chars: 0,
    note(s) { rec.notes.push(s); },
    /** A step: one thing the user does, measured. opts: expectFocus (a
        selector, or a function of the page returning bool), scroller (the
        selector whose anchor must not move; false: the step moves it on
        purpose), settle: false (don't wait to settle). */
    async step(label, act, opts = {}) {
      const s = { label, requests: [], inputs: 0 };
      const anchorSel = opts.scroller === false ? undefined : opts.scroller;
      const a0 = opts.scroller === false ? null : await anchorBefore(page, anchorSel).catch(() => null);
      const before = J.inputs;
      cur = s;
      const t0 = Date.now();
      try {
        await act();
      } catch (e) {
        s.error = String(e.message || e).split('\n')[0].slice(0, 240);
      }
      s.settleMs = opts.settle === false ? 0 : await settle(t0);
      if (unsettled) s.unsettled = unsettled;
      cur = null;
      s.inputs = J.inputs - before;
      const m = await page.evaluate((t) => window.__flow.since(t), t0).catch(() => null);
      if (m) {
        s.cls = +m.shifts.reduce((x, y) => x + y.v, 0).toFixed(4);
        s.shiftSources = [...new Set(m.shifts.flatMap((x) => x.src))].slice(0, 6);
        s.longTasks = m.longTasks.length;
        s.longTaskMs = m.longTasks.reduce((x, y) => x + y.d, 0);
        s.jankFrames = m.jank.length;
        s.worstFrame = m.jank.reduce((x, y) => Math.max(x, y.gap), 0);
        s.flicker = m.flicker.length;
        s.flickerWhat = m.flicker.slice(0, 3);
        // Shown and gone within this step (one left from the step before, or
        // an empty state the user moves away from, is no flash).
        const inStep = (x) => x.t - x.d >= t0;
        s.shortSpinners = m.spinners.filter((x) => x.d < 150 && inStep(x)).length;
        s.spinners = m.spinners.slice(0, 4);
        s.emptyFlashes = m.empties.filter((x) => x.d < 1500 && inStep(x)).length;
        s.empties = m.empties.slice(0, 4);
        s.clsSupported = m.clsSupported;
      }
      const seen = new Map();
      for (const r of s.requests) seen.set(r, (seen.get(r) ?? 0) + 1);
      s.dupRequests = [...seen.values()].reduce((x, n) => x + (n > 1 ? n - 1 : 0), 0);
      s.dups = [...seen].filter(([, n]) => n > 1).map(([r, n]) => `${r} ×${n}`).slice(0, 5);
      if (a0) {
        const a1 = await anchorAfter(page, anchorSel).catch(() => null);
        if (a1?.connected) {
          const moved = Math.round(a1.top - a0.top);
          const scrolled = Math.round(a1.scroll - a0.scroll);
          // Moved more than the user's own scroll explains.
          s.jump = Math.abs(moved + scrolled) > 4 ? moved : 0;
          if (s.jump) s.jumpWhat = a0.text;
        } else if (a1) {
          s.anchorReplaced = true;
        }
      }
      if (opts.expectFocus) {
        const ok = typeof opts.expectFocus === 'function'
          ? await opts.expectFocus(page).catch(() => false)
          : await page.evaluate((sel) => !!document.activeElement?.matches(sel), opts.expectFocus).catch(() => false);
        s.focus = await describeFocus(page);
        if (!ok) s.focusMiss = `wanted ${typeof opts.expectFocus === 'string' ? opts.expectFocus : 'the expected element'}, had ${s.focus}`;
      }
      rec.steps.push(s);
      const flags = [s.error && `ERROR ${s.error}`, s.cls > 0.01 && `CLS ${s.cls}`, s.jump && `jump ${s.jump}px`, s.flicker && `flicker ${s.flicker}`,
        s.shortSpinners && `short spinner`, s.emptyFlashes && `empty flash`, s.dupRequests && `dup ${s.dups.join(', ')}`, s.focusMiss && `focus: ${s.focusMiss}`, s.unsettled && `never settled: ${JSON.stringify(s.unsettled)}`].filter(Boolean);
      console.log(`  ${target}/${journey}/${viewport}/${browserName} ${label}: ${s.settleMs} ms, ${s.requests.length} req${flags.length ? ' — ' + flags.join('; ') : ''}`);
      if (s.error && !opts.optional) throw Object.assign(new Error(s.error), { stepFailed: true });
      return s;
    },
    /** Something the journey must keep (scroll, selection, text): a loss is counted. */
    keep(label, before, after) {
      const same = JSON.stringify(before) === JSON.stringify(after);
      const last = rec.steps[rec.steps.length - 1];
      if (last) (last.kept ||= []).push({ label, ok: same, before, after });
      if (!same) console.log(`    LOST ${label}: ${JSON.stringify(before)} → ${JSON.stringify(after)}`);
      return same;
    },
    async click(sel) { J.inputs++; await page.click(sel); },
    async tap(sel) { J.inputs++; if (J.phone) await page.tap(sel); else await page.click(sel); },
    async press(key) { J.inputs++; await page.keyboard.press(key); },
    async type(text) { J.inputs++; J.chars += text.length; await page.keyboard.type(text, { delay: 8 }); },
    async fill(sel, text) { J.inputs++; J.chars += text.length; await page.fill(sel, text); },
    async back() { J.inputs++; await page.goBack({ waitUntil: 'commit' }).catch(() => {}); },
    async shot(name) {
      const file = path.join(outDir, `${target}-${journey}-${viewport}-${browserName}-${name}.png`);
      await page.screenshot({ path: file }).catch(() => {});
      return file;
    },
  };
  try {
    if (setup) await setup(J);
    await fn(J);
  } catch (e) {
    rec.failed = String(e.message || e).split('\n')[0].slice(0, 240);
    console.log(`  ${target}/${journey}/${viewport}/${browserName} FAILED: ${rec.failed}`);
  }
  rec.shot = await J.shot('end');
  rec.chars = J.chars;
  await browser.close();
  rec.totals = totals(rec);
  return rec;
}

function totals(rec) {
  const S = rec.steps;
  const sum = (f) => S.reduce((x, s) => x + (f(s) || 0), 0);
  return {
    steps: S.length,
    settleMs: sum((s) => s.settleMs),
    worstSettleMs: S.reduce((x, s) => Math.max(x, s.settleMs || 0), 0),
    cls: sum((s) => s.cls),
    jumps: S.filter((s) => s.jump).length,
    flicker: sum((s) => s.flicker),
    shortSpinners: sum((s) => s.shortSpinners),
    emptyFlashes: sum((s) => s.emptyFlashes),
    requests: sum((s) => s.requests.length),
    dupRequests: sum((s) => s.dupRequests),
    longTasks: sum((s) => s.longTasks),
    jankFrames: sum((s) => s.jankFrames),
    inputs: sum((s) => s.inputs),
    focusMisses: S.filter((s) => s.focusMiss).length,
    lost: S.reduce((x, s) => x + (s.kept || []).filter((k) => !k.ok).length, 0),
    errors: S.filter((s) => s.error).length + (rec.failed ? 1 : 0),
  };
}

export function writeJson(file, v) { fs.writeFileSync(file, JSON.stringify(v, null, 2)); }
