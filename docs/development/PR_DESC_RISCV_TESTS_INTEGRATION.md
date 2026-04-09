Title: CI: riscv-tests integration + PerfCollector/visualize hook for cache/TLB metrics

Summary
-------
This PR adds infrastructure to run the official `riscv-tests` ISA suite in CI using the existing DiffTest harness and QEMU as the golden reference. It also extends the performance telemetry to include optional cache and TLB statistics and exposes them to the visualizer and JSON perf reports.

Changes
-------
- Add optional cache/TLB counters to `PerfCollector` and convenience API (`record_cache_hit`, etc.).
- Expose cache/TLB counters in `PerfSnapshot` so frontend visualization can show them.
- Include cache/TLB fields in `PerfReport` JSON and human-readable display.
- Ensure CI uploads per-test perf JSON artifacts for offline inspection.
- Update frontend types and dashboard to show the new fields when present.
- Update documentation and add a change log.

Why
---
- Provides visibility for cache/TLB related metrics once a cache/TLB model is available.
- Keeps CI observability (per-test perf JSON) so regressions can be diagnosed.
- Fields are optional / default to zero to maintain backwards compatibility.

Validation plan (manual / CI)
---------------------------
- CI: Run `.github/workflows/run-riscv-tests.yml` with `max_tests=10` to produce artifacts.
- Verify `artifacts/riscv-tests/*.perf.json` exists and contains `cache_hits` etc. (may be 0 if unsupported).
- Frontend: Connect to visualize server and ensure dashboard renders new fields when non-null.

Notes
-----
- I did not run the full test-suite locally because another agent is actively modifying cache/TLB code which may break the build. Please re-run final verification after those changes are merged or staged.

Reviewer guidance
-----------------
- Focus on: compatibility (fields defaulting to zero), CI artifact availability, and minimal intrusion to PerfEvent/HPM logic.
- If you prefer the cache/TLB stats to be surfaced under a separate top-level JSON object instead of MemoryStats, I can adjust accordingly.
