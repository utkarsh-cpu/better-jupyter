# Recorded evidence and local runs

This directory holds generated experiments and native UI captures. Most content
belongs only in the local workspace. The exact file allowlist in `.gitignore`
keeps the reports, measurements, capture utility source, and screenshots used by
checked-in documentation available in a fresh clone.

Retained evidence includes the baseline UI audit, documented frontend reviews,
final v6 captures and their UI Automation records, document export samples, and
the memory measurement reports. These are historical observations, not a claim
that the current checkout has passed release qualification.

Virtual environments, Rust `target/` output, process profiles, disposable
notebooks, GPU donor/recipient/export blob stores, nested experiment repositories,
and uncited intermediate captures stay local. Nothing in these local runs is
required to build the product or execute its test suite. Tests create their own
fixtures; reusable examples live under `examples/`.

When adding durable evidence, link it from the relevant document and add its
individual path to `artifacts/.gitignore`. Retain matching UI Automation data
where a screenshot supports an accessibility or interaction claim. Keep runtime
state, credentials, and dependency installations outside the allowlist.

The [repository layout](../docs/repository-layout.md) documents the full split
between Git and local files. The cleanup inventory is local at
`repository-cleanup/manifest.json`; previously tracked files remain on disk at
their original locations.
