section: Development

- **Timing budgets are set from CI's runner.** Each `timing_…` report's budget is now about 1.5x the worst figure CI's macOS runner printed for it, so the `timing` job is green until something really slows down (the rule is in `wait::Budget`'s doc). The job prints every report's figures, passing ones too (`--show-output`), and the reading report's "open a post" no longer times `settle`'s fixed pauses.
