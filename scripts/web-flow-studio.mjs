// The flow pass's journeys in the studio's own web UI, the reference for
// "nice" (scripts/browser-web-flow-pass.mjs). The same journeys as
// scripts/web-flow-burrow.mjs, done the studio's way, on a blyg seeded
// alike. Selectors are the studio's (blygger-studio src/ui).
import { runJourney } from './web-flow-metrics.mjs';
import { signIn, waitFor, newPostsOnB, spareFeed, spareName, unsubscribe, journeyList, markAllUnread, unreadOn } from './web-flow-common.mjs';

export async function studioJourneys(ctx) {
  const origin = ctx.origins.studio;
  const UI = `${origin}/studio`;
  const a = ctx.blygs.studio;
  const state = await signIn(ctx, origin);
  const out = [];
  const run = async (journey, viewport, browserName, fn, setup) => {
    if (ctx.only.length && !ctx.only.includes(journey)) return;
    console.log(`studio ${journey} (${viewport}, ${browserName})`);
    out.push(await runJourney({ browserType: ctx.pw[browserName], browserName, target: 'studio', journey, viewport, storageState: state, outDir: ctx.OUT, setup }, fn));
  };
  // The setup banner and first-run tips are dismissed before a journey (once
  // in real life): they'd cover what the journey taps.
  const quiet = async (J) => {
    await J.page.goto(`${UI}/reading`);
    await J.page.click('#setup-dismiss', { timeout: 3000 }).catch(() => {});
    await J.page.goto('about:blank');
  };
  const openReading = async (J) => {
    await J.step('open', async () => {
      await J.page.goto(`${UI}/reading`);
      await J.page.waitForSelector('article.entry', { timeout: 30_000 });
    }, { scroller: false });
  };
  const entry = (n) => `:nth-match(article.entry, ${n})`;
  const openArticle = async (J, label, n) => {
    await J.step(label, async () => {
      await J.click(`${entry(n)} a.entry-date`);
      await J.page.waitForSelector('article.reading-article', { timeout: 15_000 });
    }, { scroller: false });
  };
  const gotIt = (J) => J.page.click('button:has-text("got it")', { timeout: 1500 }).catch(() => {});

  const both = {
    'open-to-read': async (J) => {
      await openReading(J);
      await openArticle(J, 'open-post', 1);
    },
    'catch-up': async (J) => {
      await openReading(J);
      const before = await unreadOn(a);
      J.note(`unread at start (blyg): ${before}`);
      // The studio reads in the stream; opening an entry marks it read.
      for (let i = 0; i < 4; i++) {
        await openArticle(J, `open-${i + 1}`, 8 + i);
        await J.step(`back-${i + 1}`, async () => { await J.click('button.back-btn'); await J.page.waitForSelector('article.entry'); }, { scroller: false });
      }
      await J.step('read-state', () => J.click('button[aria-label="read state"]'), { scroller: false });
      await J.step('mark-all', () => J.click('text=mark all read…'), { scroller: false, expectFocus: 'button' });
      await J.step('confirm', async () => {
        await J.click('[role=dialog] button:has-text("mark read"), [role=alertdialog] button:has-text("mark read")');
        await waitFor(async () => (await unreadOn(a)) <= before * 0.05, 20_000, 'the unread to be read');
      }, { scroller: false });
    },
    'quote-publish': async (J) => {
      await openReading(J);
      await J.step('quote', async () => {
        await J.click(`${entry(9)} [data-action="stub"]`);
        await J.page.waitForSelector('#md-input', { timeout: 15_000 });
        await gotIt(J);
      }, { expectFocus: '#md-input', scroller: false });
      await J.step('type', async () => {
        if (!(await J.page.evaluate(() => document.activeElement?.id === 'md-input'))) await J.click('#md-input');
        await J.page.$eval('#md-input', (e) => e.setSelectionRange(e.value.length, e.value.length));
        await J.type('\n\nMy invented response to it.');
      }, { expectFocus: '#md-input' });
      await J.step('publish', async () => {
        await J.click('#publish-btn');
        await J.page.click('[role=dialog] button:has-text("publish")', { timeout: 2500 }).then(() => J.inputs++).catch(() => {});
        await waitFor(async () => (await a.all('/items')).some((i) => i.status === 'public' && /My invented response to it/.test(i.contentMd || '')), 15_000, 'the quote to be public');
      }, { scroller: false });
    },
    'draft-revise': async (J) => {
      const title = `Studio draft ${Date.now() % 100000}`;
      await J.step('compose', async () => { await J.page.goto(`${UI}/`); await J.page.waitForSelector('#composer-text'); }, { scroller: false });
      await J.step('focus', () => J.click('#composer-text'), { expectFocus: '#composer-text', scroller: false });
      await J.step('type', () => J.type(`${title}\n\nInvented words for a draft.`), { expectFocus: '#composer-text' });
      await J.step('save-draft', () => J.click('#save-draft-btn'), { scroller: false });
      await J.step('leave', () => J.click('a.tab[href$="/studio/reading"]'), { scroller: false });
      await J.step('back', async () => { await J.click('a.tab[href$="/studio/"]'); await J.page.waitForSelector('li.item-row'); }, { scroller: false });
      await J.step('find', async () => { await J.click(`li.item-row:has-text("${title}") button.item-head`); }, { scroller: false });
      await J.step('open', async () => {
        await J.click(`li.item-row:has-text("${title}") a:has-text("full editor")`);
        await J.page.waitForSelector('#md-input');
        await gotIt(J);
      }, { scroller: false });
      J.keep('the draft kept its words', true, (await J.page.inputValue('#md-input')).includes('Invented words for a draft.'));
      await J.step('edit', async () => { await J.click('#md-input'); await J.press('Control+End'); await J.press('End'); await J.type(' More words.'); }, { expectFocus: '#md-input' });
      await J.step('publish', async () => {
        await J.click('#publish-btn');
        await J.page.click('[role=dialog] button:has-text("publish")', { timeout: 2500 }).then(() => J.inputs++).catch(() => {});
        await waitFor(async () => (await a.all('/items')).some((i) => i.status === 'public' && (i.contentMd || '').startsWith(title)), 15_000, 'v1');
      }, { scroller: false });
      await J.step('revise', async () => { await J.click('#md-input'); await J.press('Control+End'); await J.press('End'); await J.type(' A revision.'); }, { expectFocus: '#md-input' });
      await J.step('publish-2', async () => {
        await J.click('#publish-btn');
        await J.page.click('[role=dialog] button:has-text("publish")', { timeout: 2500 }).then(() => J.inputs++).catch(() => {});
        await waitFor(async () => (await a.all('/items')).some((i) => (i.contentMd || '').startsWith(title) && i.version >= 2), 15_000, 'v2');
      }, { scroller: false });
    },
    subscribe: async (J) => {
      await openReading(J);
      await J.step('subscribe', () => J.click('button[aria-label="subscribe"]'), { expectFocus: '#add-sub-url', scroller: false });
      await J.step('address', () => J.type(J.spare), { expectFocus: '#add-sub-url', scroller: false });
      await J.step('look-up', async () => {
        await J.press('Enter');
        await J.page.waitForSelector('button:has-text("confirm subscribe")', { timeout: 15_000 });
      }, { scroller: false });
      await J.step('confirm', async () => {
        await J.click('button:has-text("confirm subscribe")');
        await J.page.waitForSelector('#add-sub-url', { state: 'detached', timeout: 15_000 });
      }, { scroller: false });
      await J.step('sources', () => J.click('button.tab:has-text("Sources")'), { scroller: false });
      await J.step('see-posts', async () => {
        await waitFor(async () => !!(await J.page.$(`li.feed:has-text("${J.spareName}")`)), 30_000, 'the new source');
        await J.click(`li.feed:has-text("${J.spareName}") .fn, li.feed:has-text("${J.spareName}") a, li.feed:has-text("${J.spareName}")`);
        await waitFor(async () => (await J.page.$$('article.entry')).length >= 5, 30_000, 'its posts');
      }, { scroller: false });
    },
    'tab-switch': async (J) => {
      await openReading(J);
      for (const round of [1, 2]) {
        for (const [k, label] of [['agentic', 'Agentic feed'], ['shelf', 'Saved'], ['background', 'Background'], ['raw', 'Raw'], ['all', 'Feed']]) {
          await J.step(`${k}-${round}`, () => J.click(`.toggle-group-item >> text="${label}"`), { scroller: false });
        }
      }
    },
    'sync-reading': async (J) => {
      await openReading(J);
      await J.step('scroll-list', async () => { J.inputs++; await J.page.evaluate(() => window.scrollBy(0, 1400)); await J.sleep(300); }, { scroller: false });
      await J.step('expand-mid', async () => {
        const more = await J.page.$$('button.expand-btn');
        for (const m of more) { const r = await m.boundingBox(); if (r && r.y > 150 && r.y < 800) { J.inputs++; await m.click(); break; } }
      }, { scroller: false });
      await J.page.evaluate(() => {
        const p = [...document.querySelectorAll('article.entry .entry-body p, article.entry .entry-body')].find((e) => { const r = e.getBoundingClientRect(); return r.top > 100 && r.top < 600; });
        if (!p) return;
        const r = document.createRange(); r.selectNodeContents(p); const s = getSelection(); s.removeAllRanges(); s.addRange(r);
      });
      const keep = () => J.page.evaluate(() => ({ selection: getSelection().toString().slice(0, 40) }));
      const k0 = await keep();
      await newPostsOnB(ctx, a, 3);
      await J.step('sync-arrives', async () => {
        await J.page.evaluate(() => { document.dispatchEvent(new Event('visibilitychange')); window.dispatchEvent(new Event('focus')); });
        await J.sleep(4000);
      });
      J.keep('the place and the selection', k0, await keep());
    },
    'sync-typing': async (J) => {
      await J.step('compose', async () => { await J.page.goto(`${UI}/`); await J.page.waitForSelector('li.item-row'); }, { scroller: false });
      await J.step('open-draft', async () => {
        await J.click('li.item-row:has-text("Unfinished draft") button.item-head');
        await J.click('li.item-row:has-text("Unfinished draft") a:has-text("full editor")');
        await J.page.waitForSelector('#md-input');
        await gotIt(J);
      }, { scroller: false });
      await J.step('type-1', async () => { await J.click('#md-input'); await J.press('Control+End'); await J.press('End'); await J.type(' First typed words.'); }, { expectFocus: '#md-input' });
      await newPostsOnB(ctx, a, 2);
      await J.step('type-during-sync', async () => {
        const typing = J.type(' Typed while the blyg answers.');
        await J.page.evaluate(() => { document.dispatchEvent(new Event('visibilitychange')); window.dispatchEvent(new Event('focus')); });
        await typing;
        await J.sleep(3000);
      }, { expectFocus: '#md-input' });
      const v = await J.page.inputValue('#md-input');
      J.keep('every typed word', true, v.includes('First typed words. Typed while the blyg answers.'));
      J.keep('the caret at the end', true, await J.page.$eval('#md-input', (e) => e.selectionStart === e.value.length));
    },
  };
  const phoneOnly = {
    'back-and-scroll': async (J) => {
      await openReading(J);
      await J.step('scroll-stream', async () => { J.inputs++; await J.page.evaluate(() => window.scrollBy(0, 1500)); await J.sleep(300); }, { scroller: false });
      const where = () => J.page.evaluate(() => {
        const first = [...document.querySelectorAll('article.entry')].find((c) => c.getBoundingClientRect().bottom > 60);
        return { key: first?.querySelector('a.entry-date')?.getAttribute('href') ?? null, offset: first ? Math.round(first.getBoundingClientRect().top) : null };
      });
      const w0 = await where();
      const openMid = async () => {
        const links = await J.page.$$('article.entry a.entry-date');
        for (const l of links) { const r = await l.boundingBox(); if (r && r.y > 200 && r.y < 700) { J.inputs++; await l.tap(); break; } }
        await J.page.waitForSelector('article.reading-article', { timeout: 15_000 });
      };
      await J.step('open-card', openMid, { scroller: false });
      await J.step('scroll-post', async () => { J.inputs++; await J.page.evaluate(() => window.scrollBy(0, 500)); }, { scroller: false });
      await J.step('browser-back', async () => { await J.back(); await J.page.waitForSelector('article.entry:not(.reading-article)'); await J.sleep(500); }, { scroller: false });
      J.keep('the stream where it was (browser back)', w0, await where());
      await J.step('open-again', openMid, { scroller: false });
      await J.step('app-back', async () => { await J.tap('button.back-btn'); await J.page.waitForSelector('article.entry:not(.reading-article)'); await J.sleep(500); }, { scroller: false });
      J.keep('the stream where it was (back)', w0, await where());
      await J.step('refresh', async () => { J.inputs++; await J.page.reload(); await J.page.waitForSelector('article.entry'); }, { scroller: false });
      J.keep('the Reading page after a reload', true, /\/studio\/reading/.test(J.page.url()));
    },
    keyboard: async (J) => {
      await J.step('compose', async () => { await J.page.goto(`${UI}/`); await J.page.waitForSelector('#composer-text'); }, { scroller: false });
      await J.step('keyboard-opens', async () => { await J.tap('#composer-text'); await J.page.setViewportSize({ width: 390, height: 500 }); }, { expectFocus: '#composer-text', scroller: false });
      const visible = (sel) => J.page.$eval(sel, (e) => { const r = e.getBoundingClientRect(); return r.top >= 0 && r.bottom <= innerHeight + 1 && r.height > 0; }).catch(() => false);
      await J.step('type', () => J.type('Typed with the keyboard up.'), { expectFocus: '#composer-text' });
      J.keep('the text box on screen', true, await visible('#composer-text'));
      J.keep('Publish above the keyboard', true, await visible('#publish-btn'));
      await J.step('keyboard-closes', () => J.page.setViewportSize({ width: 390, height: 844 }), { scroller: false });
    },
  };

  const spare = spareFeed(ctx);
  const subSetup = async (J) => { await unsubscribe(a, spare); J.spare = spare; J.spareName = spareName(ctx); await quiet(J); };
  for (const name of journeyList(ctx)) {
    const setup = name === 'subscribe' ? subSetup : name === 'catch-up' ? async (J) => { await markAllUnread(a); await quiet(J); } : quiet;
    if (both[name]) await run(name, 'desktop', 'chromium', both[name], setup);
    const ph = both[name] ?? phoneOnly[name];
    if (ph) for (const br of ctx.browsers) await run(name, 'phone', br, ph, setup);
  }
  return out;
}
