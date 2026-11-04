# Repository and local workspace

Keep the files needed to build, test, understand, and reproduce KernelLedger in
Git. Keep installed tools, generated runs, and machine-specific state local.

## Versioned project files

| Location | Purpose |
| --- | --- |
| `src/`, `desktop/crates/` | Python runtime and Rust desktop source |
| `tests/`, `tests/fixtures/`, `schemas/` | Regression tests, curated fixtures, and persisted contracts |
| `examples/`, `scripts/` | Runnable examples and development/qualification tools |
| `pyproject.toml`, `uv.lock`, `desktop/Cargo.toml`, `desktop/Cargo.lock` | Dependency definitions and reproducible resolutions |
| `desktop/rust-toolchain.toml`, `desktop/.cargo/`, `.github/` | Shared compiler settings and CI |
| `README.md`, `PLAN.md`, `FRONTEND_PLAN.md`, `plans/`, `docs/` | Usage, current plans, plan archives, and implementation records |
| `benchmarks/`, selected files in `artifacts/` | Recorded measurements and evidence cited by documentation |

Historical UI reports are indexed under [desktop reviews](desktop/reviews/README.md).
Plans retain their existing versioned archive structure.

## Files that stay local

- `.venv/`, `.venv-*/`, and nested `venv/`: installed Python environments.
- Any `target/`, `build/`, or `dist/`: compiler and package output.
- `.pytest_cache/`, `.ruff_cache/`, `.hypothesis/`, `__pycache__/`, and coverage
  output: reproducible caches and test results.
- `.kl/`, `.kl-gpu-experimental/`, and `.kl-pyright.json`: notebook runtime data,
  checkpoints, credentials, preferences, and generated interpreter configuration.
- Root `analysis/`: the scratch notebook used by the README quick start. Shared
  examples belong under `examples/`.
- `.agents/`, `.claude/`, and `skills-lock.json`: installed assistant skills and
  their installation metadata. They remain usable in this checkout.
- `.env` files, editor settings, and root CI qualification output: workstation or
  run-specific data. Sanitized `.env.example` and `.env.template` may be shared.
- Other `artifacts/` content: disposable notebooks, GPU blob stores, copied
  source, nested experiment repositories, profiles, screenshots, and run logs.

The [artifact policy](../artifacts/README.md) explains how cited evidence is
selected. Root `.gitignore` handles common local files; `artifacts/.gitignore`
lists the individual evidence files kept in Git. New generated runs are ignored
by default. Notebook projects created outside root `analysis/` are not ignored
wholesale: their source may be intentionally versioned, while `.kl/` stays local.

## Working with this layout

Use the [README](../README.md) development commands to install dependencies and
run checks. Environments and build directories can be recreated from the checked-in
manifests; experiment state and unpublished notebooks may be valuable and should
be backed up before removal. Generated output can remain at its current path so
existing local workflows continue to work.

Check that an intended local file is ignored with `git check-ignore -v <path>`.
Before committing, `git ls-files -ci --exclude-standard` should produce no output:
it detects ignored files that have accidentally remained tracked.

This cleanup removes local-only files from the Git index while preserving their
working copies. A local inventory, including original index entries, is saved at
`artifacts/repository-cleanup/manifest.json`. These index removals are staged so
the cleanup can be reviewed and committed. Existing Git history still contains
the old files; this cleanup does not rewrite history or shrink `.git` storage.
