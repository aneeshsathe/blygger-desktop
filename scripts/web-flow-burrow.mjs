// The flow pass's journeys in Burrow web (scripts/browser-web-flow-pass.mjs).
// Each journey is what a person does, one measured step per input; the same
// journeys run in the studio's UI (scripts/web-flow-studio.mjs).
import { runJourney } from './web-flow-metrics.mjs';
import { signIn, waitFor, newPostsOnB, spareFeed, spareName, unsubscribe, journeyList, markAllUnread, unreadOn } from './web-flow-common.mjs';

const M = 'Control';

export async function burrowJourneys(ctx) {
  const origin = ctx.origins.burrow;
  const APP = `${origin}/burrow/`;
  const a = ctx.blygs.burrow;
  const state = await signIn(ctx, origin);
  const out = [];
  const run = async (journey, viewport, browserName, fn, setup) => {
    if (ctx.only.length && !ctx.only.includes(journey)) return;
    console.log(`burrow ${journey} (${viewport}, ${browserName})`);
    out.push(await runJourney({ browserType: ctx.pw[browserName], browserName, target: 'burrow', journey, viewport, storageState: state, outDir: ctx.OUT, setup }, fn));
  };

  // ---------------------------------------------------------------- desk
  const openDesk = async (J) => {
    await J.step('open', async () => {
      await J.page.goto(APP);
      await J.page.waitForSelector('#list .row', { timeout: 30_000 });
    }, { scroller: false });
  };
  const deskReading = async (J) => {
    await J.step('reading', async () => {
      await J.click('.views [data-view="reading"]');
      await J.page.waitForSelector('#d-stream .card', { timeout: 30_000 });
    }, { scroller: false });
  };
  const unread = (J) => J.page.$eval('#d-unread', (e) => +(e.textContent || '0')).catch(() => -1);

  const desk = {
    'open-to-read': async (J) => {
      await openDesk(J);
      await deskReading(J);
      await J.step('open-post', async () => {
        await J.click('#d-stream .card');
        await J.page.waitForFunction(() => (document.querySelector('#d-post')?.innerText || '').length > 40);
      }, { scroller: '#d-stream' });
    },
    'catch-up': async (J) => {
      await openDesk(J);
      await deskReading(J);
      const before = await unread(J);
      J.note(`unread at start: ${before}`);
      await J.step('open-first', () => J.click('#d-stream .card'), { scroller: '#d-stream' });
      for (let i = 0; i < 3; i++) await J.step(`next-${i + 1}`, () => J.click('#d-ractions [data-ra="next"]'), { scroller: '#d-stream' });
      await J.step('mark-all', () => J.click('#d-wr-sources [data-go="markall"]'), { expectFocus: '[data-markall-go]', scroller: false });
      await J.step('confirm', async () => {
        await J.press('Enter');
        await waitFor(async () => (await unread(J)) <= before * 0.05, 15_000, 'the unread count to fall');
        await waitFor(async () => (await unreadOn(a)) <= before * 0.05, 20_000, 'the blyg to have them read');
      }, { scroller: '#d-stream' });
    },
    'quote-publish': async (J) => {
      await openDesk(J);
      await deskReading(J);
      await J.step('open-post', () => J.click('#d-stream .card:nth-child(2)'), { scroller: '#d-stream' });
      await J.step('quote', async () => {
        await J.click('#d-ractions [data-ra="quote"]');
        await J.page.waitForFunction(() => /!\[\[/.test(document.querySelector('#ed')?.value || ''));
      }, { expectFocus: '#ed', scroller: false });
      await J.step('type', () => J.type('\n\nMy invented response to it.'), { expectFocus: '#ed' });
      await J.step('publish-sheet', () => J.press(`${M}+Enter`), { expectFocus: '#sheet input, #sheet textarea, #sheet button', scroller: false });
      await J.step('publish', async () => {
        await J.press('Enter');
        await waitFor(async () => (await a.all('/items')).some((i) => i.status === 'public' && /My invented response to it/.test(i.contentMd || '')), 15_000, 'the quote to be public');
      });
    },
    'draft-revise': async (J) => {
      const title = `Flow draft ${Date.now() % 100000}`;
      await openDesk(J);
      await J.step('omnibar', () => J.click('#omni'), { expectFocus: '#omni', scroller: false });
      await J.step('title', () => J.type(title), { expectFocus: '#omni', scroller: false });
      await J.step('create', () => J.press('Enter'), { expectFocus: '#ed', scroller: false });
      await J.step('type', () => J.type('\n\nInvented words for a draft.'), { expectFocus: '#ed' });
      await J.step('leave', () => J.click('.views [data-view="reading"]'), { scroller: false });
      await J.step('back', () => J.click('.views [data-view="posts"]'), { scroller: false });
      await J.step('find', () => J.fill('#omni', title), { scroller: false });
      await J.step('open', () => J.press('Enter'), { expectFocus: '#ed', scroller: false });
      J.keep('the draft kept its words', true, (await J.page.inputValue('#ed')).includes('Invented words for a draft.'));
      await J.step('edit', async () => { await J.press('End'); await J.type(' More words.'); }, { expectFocus: '#ed' });
      await J.step('publish-sheet', () => J.press(`${M}+Enter`), { scroller: false });
      await J.step('publish', async () => {
        await J.press('Enter');
        await waitFor(async () => (await a.all('/items')).some((i) => i.status === 'public' && (i.contentMd || '').startsWith(title)), 15_000, 'v1');
      }, { scroller: false });
      await J.step('revise', async () => { await J.click('#ed'); await J.press(`${M}+End`); await J.type(' A revision.'); }, { expectFocus: '#ed' });
      await J.step('publish-sheet-2', () => J.press(`${M}+Enter`), { scroller: false });
      await J.step('publish-2', async () => {
        await J.press('Enter');
        await waitFor(async () => (await a.all('/items')).some((i) => (i.contentMd || '').startsWith(title) && i.version >= 2), 15_000, 'v2');
      }, { scroller: false });
    },
    subscribe: async (J) => {
      await openDesk(J);
      await deskReading(J);
      await J.step('subscribe', () => J.click('#d-wr-sources [data-go="subscribe"]'), { expectFocus: '#wr-body input[name="url"]', scroller: false });
      await J.step('address', () => J.type(J.spare), { expectFocus: '#wr-body input[name="url"]', scroller: false });
      await J.step('look-up', async () => {
        await J.press('Enter');
        await J.page.waitForSelector('#wr-body [data-sub]', { timeout: 15_000 });
      }, { expectFocus: '#wr-body input[name="title"]', scroller: false });
      await J.step('confirm', async () => {
        await J.press('Enter');
        await J.page.waitForSelector('#wr-body [data-sub]', { state: 'detached', timeout: 15_000 });
      }, { scroller: false });
      await J.step('see-posts', async () => {
        await waitFor(async () => !!(await J.page.$(`#d-wr-sources [data-src^="sub:"] >> text=${J.spareName}`)), 30_000, 'the new source');
        await J.click(`#d-wr-sources [data-src^="sub:"] >> text=${J.spareName}`);
        await waitFor(async () => (await J.page.$$('#d-stream .card')).length >= 5, 30_000, 'its posts');
      }, { scroller: false });
    },
    'tab-switch': async (J) => {
      await openDesk(J);
      await deskReading(J);
      for (const round of [1, 2]) {
        for (const src of ['agentic', 'shelf', 'background', 'raw', 'all']) {
          await J.step(`${src}-${round}`, () => J.click(`#d-wr-sources [data-src="smart:${src}"]`), { scroller: false });
        }
      }
    },
    'sync-reading': async (J) => {
      await openDesk(J);
      await deskReading(J);
      await J.step('scroll-list', async () => { J.inputs++; await J.page.$eval('#d-stream', (e) => e.scrollBy(0, 1400)); }, { scroller: false });
      await J.step('open-mid', async () => {
        const cards = await J.page.$$('#d-stream .card');
        const box = await J.page.$eval('#d-stream', (e) => e.getBoundingClientRect().top);
        for (const c of cards) { const r = await c.boundingBox(); if (r && r.y > box + 100) { J.inputs++; await c.click(); break; } }
      }, { scroller: false });
      await J.step('scroll-post', async () => { J.inputs++; await J.page.$eval('#d-rpane .rscroll', (e) => e.scrollBy(0, 600)); }, { scroller: false });
      await J.page.evaluate(() => {
        const p = document.querySelector('#d-post p'); if (!p) return;
        const r = document.createRange(); r.selectNodeContents(p); const s = getSelection(); s.removeAllRanges(); s.addRange(r);
      });
      const keep = () => J.page.evaluate(() => ({
        cur: document.querySelector('#d-stream .card.cur')?.dataset.key ?? null,
        post: (document.querySelector('#d-post')?.innerText || '').slice(0, 60),
        postScroll: Math.round(document.querySelector('#d-rpane .rscroll')?.scrollTop ?? -1),
        selection: getSelection().toString().slice(0, 40),
      }));
      const k0 = await keep();
      await newPostsOnB(ctx, a, 3);
      await J.step('sync-arrives', async () => {
        // The user comes back to the tab: the app checks the blyg.
        await J.page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
        await waitFor(async () => (await J.page.$$eval('#d-stream .card', (cs) => cs.filter((c) => /Late invented note/.test(c.innerText)).length)) >= 1, 30_000, 'the new posts in the list');
      }, { scroller: '#d-stream' });
      J.keep('reader, its scroll and the selection', k0, await keep());
    },
    'sync-typing': async (J) => {
      await openDesk(J);
      await J.step('open-draft', async () => { await J.fill('#omni', 'Unfinished draft'); await J.press('Enter'); }, { expectFocus: '#ed', scroller: false });
      await J.step('type-1', async () => { await J.press(`${M}+End`); await J.type(' First typed words.'); }, { expectFocus: '#ed' });
      await newPostsOnB(ctx, a, 2);
      await J.step('type-during-sync', async () => {
        const typing = J.type(' Typed while the blyg answers.');
        await J.page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
        await typing;
        await J.sleep(3000); // the save goes out after typing stops
      }, { expectFocus: '#ed' });
      const v = await J.page.inputValue('#ed');
      J.keep('every typed word', true, v.includes('First typed words. Typed while the blyg answers.'));
      const caret = await J.page.$eval('#ed', (e) => e.selectionStart === e.value.length);
      J.keep('the caret at the end', true, caret);
    },
  };

  // ---------------------------------------------------------------- phone
  const openPhone = async (J) => {
    await J.step('open', async () => {
      await J.page.goto(APP);
      await J.page.waitForFunction(() => document.querySelector('#p-loading.gone') && document.querySelectorAll('#p-list .prow').length > 0, null, { timeout: 30_000 });
    }, { scroller: false });
  };
  const phoneRead = async (J) => {
    await J.step('read-tab', async () => {
      await J.tap('.tab[data-tab="read"]');
      await J.page.waitForSelector('#p-stream .card', { timeout: 30_000 });
    }, { scroller: false });
  };
  const phoneUnread = (J) => J.page.$eval('#p-tabn', (e) => +(e.textContent || '0')).catch(() => -1);
  const phone = {
    'open-to-read': async (J) => {
      await openPhone(J);
      await phoneRead(J);
      await J.step('open-post', async () => {
        await J.tap('#p-stream .card');
        await J.page.waitForFunction(() => (document.querySelector('#p-postBody')?.innerText || '').length > 40);
      }, { scroller: false });
    },
    'catch-up': async (J) => {
      await openPhone(J);
      await phoneRead(J);
      const before = await phoneUnread(J);
      J.note(`unread at start: ${before}`);
      await J.step('open-first', () => J.tap('#p-stream .card'), { scroller: false });
      for (let i = 0; i < 3; i++) await J.step(`next-${i + 1}`, () => J.tap('#p-nextPost'), { scroller: false });
      await J.step('back', () => J.tap('#p-back2'), { scroller: false });
      await J.step('mark-all', async () => {
        await J.page.$eval('#p-wrBar', (e) => e.scrollTo(1e4, 0));
        await J.tap('#p-wrBar [data-go="markall"]');
      }, { scroller: false });
      await J.step('confirm', async () => {
        await J.tap('[data-markall-go]');
        await waitFor(async () => (await phoneUnread(J)) <= before * 0.05, 15_000, 'the unread count to fall');
        await waitFor(async () => (await unreadOn(a)) <= before * 0.05, 20_000, 'the blyg to have them read');
      }, { scroller: false });
    },
    'quote-publish': async (J) => {
      await openPhone(J);
      await phoneRead(J);
      await J.step('open-post', () => J.tap('#p-stream .card:nth-child(2)'), { scroller: false });
      await J.step('quote', async () => {
        await J.tap('#p-pacts [data-pa="quote"]');
        await J.page.waitForFunction(() => /!\[\[/.test(document.querySelector('#p-ta')?.value || ''));
      }, { expectFocus: '#p-ta', scroller: false });
      await J.step('type', async () => { await J.page.focus('#p-ta'); await J.type('\n\nMy invented response from a phone.'); }, { expectFocus: '#p-ta' });
      await J.step('publish-sheet', () => J.tap('#p-bar [data-tool="publish"]'), { scroller: false });
      await J.step('publish', async () => {
        await J.tap('#p-pubGo');
        await waitFor(async () => (await a.all('/items')).some((i) => i.status === 'public' && /from a phone/.test(i.contentMd || '')), 15_000, 'the quote to be public');
      }, { scroller: false });
    },
    'draft-revise': async (J) => {
      const title = `Phone draft ${Date.now() % 100000}`;
      await openPhone(J);
      await J.step('search', () => J.tap('#p-q'), { expectFocus: '#p-q', scroller: false });
      await J.step('title', () => J.type(title), { scroller: false });
      await J.step('start', () => J.tap('#p-list [data-new]'), { expectFocus: '#p-ta', scroller: false });
      await J.step('type', async () => { await J.page.focus('#p-ta'); await J.type('\n\nInvented words for a draft.'); }, { expectFocus: '#p-ta' });
      await J.step('leave', () => J.tap('#p-back'), { scroller: false });
      await J.step('back', () => J.tap(`#p-list .prow >> text=${title}`), { scroller: false });
      J.keep('the draft kept its words', true, (await J.page.inputValue('#p-ta')).includes('Invented words for a draft.'));
      await J.step('edit', async () => { await J.tap('#p-ta'); await J.page.$eval('#p-ta', (e) => e.setSelectionRange(e.value.length, e.value.length)); await J.type(' More.'); }, { expectFocus: '#p-ta' });
      await J.step('publish-sheet', () => J.tap('#p-bar [data-tool="publish"]'), { scroller: false });
      await J.step('publish', async () => {
        await J.tap('#p-pubGo');
        await waitFor(async () => (await a.all('/items')).some((i) => i.status === 'public' && (i.contentMd || '').startsWith(title)), 15_000, 'v1');
      }, { scroller: false });
      await J.step('revise', async () => { await J.tap('#p-ta'); await J.page.$eval('#p-ta', (e) => e.setSelectionRange(e.value.length, e.value.length)); await J.type(' A revision.'); }, { expectFocus: '#p-ta' });
      await J.step('publish-sheet-2', () => J.tap('#p-bar [data-tool="publish"]'), { scroller: false });
      await J.step('publish-2', async () => {
        await J.tap('#p-pubGo');
        await waitFor(async () => (await a.all('/items')).some((i) => (i.contentMd || '').startsWith(title) && i.version >= 2), 15_000, 'v2');
      }, { scroller: false });
    },
    subscribe: async (J) => {
      await openPhone(J);
      await phoneRead(J);
      await J.step('subscribe', () => J.tap('#p-wrBar [data-go="subscribe"]'), { expectFocus: '#wr-body input[name="url"]', scroller: false });
      await J.step('address', () => J.type(J.spare), { scroller: false });
      await J.step('look-up', async () => { await J.press('Enter'); await J.page.waitForSelector('#wr-body [data-sub]', { timeout: 15_000 }); }, { scroller: false });
      await J.step('confirm', async () => { await J.tap('#wr-body [data-sub]'); await J.page.waitForSelector('#wr-body [data-sub]', { state: 'detached', timeout: 15_000 }); }, { scroller: false });
      await J.step('see-posts', async () => {
        await J.tap('#p-wrBar [data-go="sources"]');
        await waitFor(async () => !!(await J.page.$(`#wr-body [data-src^="sub:"] >> text=${J.spareName}`)), 30_000, 'the new source');
        await J.tap(`#wr-body [data-src^="sub:"] >> text=${J.spareName}`);
        await waitFor(async () => (await J.page.$$('#p-stream .card')).length >= 5, 30_000, 'its posts');
      }, { scroller: false });
    },
    'tab-switch': async (J) => {
      await openPhone(J);
      await phoneRead(J);
      for (const round of [1, 2]) {
        for (const src of ['agentic', 'shelf', 'background', 'raw', 'all']) {
          await J.step(`${src}-${round}`, async () => {
            await J.tap('#p-wrBar [data-go="sources"]');
            await J.tap(`#wr-body [data-src="smart:${src}"]`);
          }, { scroller: false });
        }
      }
    },
    'back-and-scroll': async (J) => {
      await openPhone(J);
      await phoneRead(J);
      await J.step('scroll-stream', async () => { J.inputs++; await J.page.$eval('#p-stream', (e) => e.scrollBy(0, 1500)); }, { scroller: false });
      const where = () => J.page.evaluate(() => {
        const s = document.querySelector('#p-stream');
        const top = s.getBoundingClientRect().top;
        const first = [...s.querySelectorAll('.card')].find((c) => c.getBoundingClientRect().bottom > top + 1);
        return { key: first?.dataset.key ?? null, offset: first ? Math.round(first.getBoundingClientRect().top - top) : null };
      });
      const w0 = await where();
      await J.step('open-card', async () => {
        const cards = await J.page.$$('#p-stream .card');
        for (const c of cards) { const r = await c.boundingBox(); if (r && r.y > 250 && r.y < 700) { J.inputs++; await c.tap(); break; } }
        await J.page.waitForFunction(() => document.querySelector('#phone')?.dataset.view === 'post');
      }, { scroller: false });
      await J.step('scroll-post', async () => { J.inputs++; await J.page.$eval('#v-post .pscroll', (e) => e.scrollBy(0, 500)); }, { scroller: false });
      await J.step('browser-back', async () => { await J.back(); await J.page.waitForFunction(() => document.querySelector('#phone')?.dataset.view === 'read'); }, { scroller: false });
      J.keep('the stream where it was (browser back)', w0, await where());
      await J.step('open-again', async () => {
        const cards = await J.page.$$('#p-stream .card');
        for (const c of cards) { const r = await c.boundingBox(); if (r && r.y > 250 && r.y < 700) { J.inputs++; await c.tap(); break; } }
        await J.page.waitForFunction(() => document.querySelector('#phone')?.dataset.view === 'post');
      }, { scroller: false });
      await J.step('app-back', async () => { await J.tap('#p-back2'); await J.page.waitForFunction(() => document.querySelector('#phone')?.dataset.view === 'read'); }, { scroller: false });
      J.keep('the stream where it was (‹ Reading)', w0, await where());
      await J.step('refresh', async () => {
        J.inputs++;
        await J.page.reload();
        await J.page.waitForFunction(() => document.querySelector('#p-loading.gone'), null, { timeout: 30_000 });
      }, { scroller: false });
      J.keep('the Read tab after a reload', 'read', await J.page.evaluate(() => document.querySelector('#phone')?.dataset.view));
    },
    keyboard: async (J) => {
      await openPhone(J);
      await J.step('open-post', () => J.tap('#p-list .prow'), { scroller: false });
      // The on-screen keyboard: the visual viewport shrinks to 500 px.
      await J.step('keyboard-opens', async () => {
        await J.tap('#p-ta');
        await J.page.setViewportSize({ width: 390, height: 500 });
      }, { expectFocus: '#p-ta', scroller: false });
      const visible = (sel) => J.page.$eval(sel, (e) => { const r = e.getBoundingClientRect(); return r.top >= 0 && r.bottom <= innerHeight + 1 && r.height > 0; }).catch(() => false);
      J.keep('the editor bar above the keyboard', true, await visible('#p-bar'));
      await J.step('type', async () => { await J.page.$eval('#p-ta', (e) => e.setSelectionRange(e.value.length, e.value.length)); await J.type(' Typed with the keyboard up.'); }, { expectFocus: '#p-ta' });
      const caretSeen = await J.page.$eval('#p-ta', (e) => e.getBoundingClientRect().bottom <= innerHeight + 1 && e.getBoundingClientRect().top >= 0);
      J.keep('the text box on screen', true, caretSeen);
      await J.step('keyboard-closes', async () => { await J.page.setViewportSize({ width: 390, height: 844 }); }, { scroller: false });
      await J.step('capture', async () => {
        await J.tap('#p-back');
        await J.tap('#p-fab');
        await J.page.setViewportSize({ width: 390, height: 500 });
      }, { expectFocus: '#p-capTa', scroller: false });
      J.keep('the capture box above the keyboard', true, await visible('#p-capTa'));
      J.keep('Publish above the keyboard', true, await visible('#p-capPub'));
    },
  };

  const spare = spareFeed(ctx);
  const subSetup = async (J) => { await unsubscribe(a, spare); J.spare = spare; J.spareName = spareName(ctx); };
  const setupOf = (name) => (name === 'subscribe' ? subSetup : name === 'catch-up' ? async () => { await markAllUnread(a); } : undefined);
  for (const name of journeyList(ctx)) {
    if (desk[name]) await run(name, 'desktop', 'chromium', desk[name], setupOf(name));
    if (phone[name]) {
      for (const br of ctx.browsers) await run(name, 'phone', br, phone[name], setupOf(name));
    }
  }
  return out;
}
