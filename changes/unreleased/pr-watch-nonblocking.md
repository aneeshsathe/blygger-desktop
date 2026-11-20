section: Development

- **`scripts/pr-watch.sh` reads non-blocking jobs as non-blocking.** A failed job with `continue-on-error: true` (the timing reports) is listed when the run finishes but doesn't fail the watch or stop `--merge`, as GitHub doesn't fail the run for it.
