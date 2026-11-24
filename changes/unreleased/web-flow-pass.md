section: Development

- **Web: the flow pass.** `scripts/browser-web-flow-pass.sh` runs ten
  journeys (first open to reading, catching up, quote and publish, a draft
  revised, subscribe, switching sources, a sync while reading and while
  typing, phone back and scroll, the phone keyboard) at desktop and phone
  size in Chromium and WebKit, on Burrow web and on the studio's UI, against
  local studios with 300+ invented posts. Each step is timed to a quiet page
  and checked for layout shift, flicker, short spinners, empty flashes,
  duplicate requests, focus and lost text or scroll; Burrow's numbers must
  stay within `scripts/web-flow-budgets.json`.
