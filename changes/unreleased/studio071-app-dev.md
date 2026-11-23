section: Development

- **Core: generating on the blyg keeps its provenance here.**
  `Live::generate_on_blyg` adopts the blyg's recorded provenance for the
  new text, so the writer lists what it read and later edits carry it
  (`crates/blyg-core/src/live.rs`; e2e `studio071.rs`).
- **Scenes:** `ai-checkpoint`, `ai-checkpoint-details`, `ai-tk-read`,
  `ai-tk-read-preview`, `settings-count-read`, `rd-stream-date`,
  `rd-gen-read`. The fake blyg's `without_reader_mark_read()` stands for a
  blyg before studio 0.70; a reading perf step measures the seen batch.
