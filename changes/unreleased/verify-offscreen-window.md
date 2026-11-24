section: Development

- **Verify: shots skip off-screen windows.** `scripts/verify/winid.swift`
  lists only on-screen windows. A run with a web view also owns a blank
  500×500 pt window that is never on screen, which the blank-shot check
  failed on (`rd-raw`, `rd-feed`, `rd-versions`, `rd-stream-copies`, …).
  Users never see that window.
