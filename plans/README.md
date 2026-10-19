# Plan versioning

KernelLedger has two plan tracks. Each track has exactly one current document at the repository root and an archive of superseded versions in this directory.

| Track | Current document | Archive | Owns |
| --- | --- | --- | --- |
| Runtime | [`PLAN.md`](../PLAN.md) | `plans/vN.md` | Guarantees, tracked and local file formats, ledger schema, worker, legacy and desktop protocols, desktop helper, CLI, runtime phases. |
| Frontend | [`FRONTEND_PLAN.md`](../FRONTEND_PLAN.md) | `plans/frontend-vN.md` | Desktop application: UI stack, layout, interaction, editing UX, output rendering, accessibility, desktop packaging and release, frontend phases. |

Runtime archives keep their historical names (`v1.md`, `v2.md`, `v3.md`) so existing links stay valid.

## Index

| Track | Version | Status | Document |
| --- | --- | --- | --- |
| Runtime | 1 | Superseded 2026-10-03 | [v1.md](v1.md) |
| Runtime | 2 | Superseded 2026-10-03 | [v2.md](v2.md) |
| Runtime | 3 | Superseded 2026-10-04 | [v3.md](v3.md) |
| Runtime | 4 | Superseded 2026-10-08 | [v4.md](v4.md) |
| Runtime | 5 | Current; bounded training integration | [PLAN.md](../PLAN.md) |
| Runtime | 5 | Draft 2026-10-07; GPU experiments, native snapshots, Git/worktrees, Windows/Linux targets | [v5-draft.md](v5-draft.md) |
| Frontend | 1 | Superseded 2026-10-04 | [frontend-v1.md](frontend-v1.md) |
| Frontend | 2 | Superseded 2026-10-05 | [frontend-v2.md](frontend-v2.md) |
| Frontend | 3 | Superseded 2026-10-08 | [frontend-v3.md](frontend-v3.md) |
| Frontend | 4 | Current, requires runtime 5 | [FRONTEND_PLAN.md](../FRONTEND_PLAN.md) |
| Frontend | 5 | Draft 2026-10-08; complete desktop visual and interaction redesign | [frontend-v5-draft.md](frontend-v5-draft.md) |
| Frontend | 6 | Draft 2026-10-09; clarity implementation in working tree; runtime and release gates pending | [frontend-v6-draft.md](frontend-v6-draft.md) |
| Both | — | Draft 2026-10-20; optional AI assistant, proposed for runtime 6 and the next frontend version | [ai-assistant-draft.md](ai-assistant-draft.md) |

The [runtime version 5 draft](v5-draft.md) covers portable GPU training state,
warm workers, NVIDIA/AMD Linux process snapshots, commit/worktree association,
native Windows, WSL2, and Linux execution controlled from Windows. It is not yet
authoritative; runtime version 5 and frontend version 4 are current for the bounded integration in runtime section 24.

## Rules

1. **Version numbers.** Each track numbers its versions with positive integers, independently of the other track. Any change to a guarantee, decision, requirement, contract, phase, or exit gate publishes a new version. Fixing a typo, a broken link, or wording that changes no meaning is an in-place edit and needs no version.
2. **Publishing a version.**
   1. Copy the current document verbatim to the archive path.
   2. Prepend the archive header: a title with the version, `**Status:** Superseded on <date> by <link>`, an `**Archived summary:**` paragraph, the sentence "The text below is the version N plan as it stood when version N+1 replaced it.", and a `---` rule.
   3. Rewrite the archived document's relative links so they resolve from `plans/`. Point a link to "the current runtime plan" at the archived runtime version it meant at the time.
   4. In the current document, update the header table (version, date, status, supersedes), add a version history row, and replace the "Changes from version N−1" section with one that lists every change. Earlier change tables stay in their archives.
   5. Update this index and any README links.
3. **Statuses.** `Draft` means under review and not authoritative, kept as `plans/<track>-vN-draft.md` (runtime drafts use `plans/vN-draft.md`). `Current` means authoritative; there is one per track. `Superseded` means archived. A `Current` header also states how much is implemented. Planned additions in a current runtime plan are marked **(vN)** inline until they ship. The marker is then removed by an in-place edit, because shipping changes no decision.
4. **Ownership across tracks.** The frontend plan may not define runtime behavior, persisted formats, protocol messages, or CLI behavior. It cites runtime plan sections. When the frontend needs a new or changed contract, the runtime plan publishes a version first, or a runtime draft is accepted together with the frontend version that needs it.
5. **Dependencies.** The frontend header has a `Requires` row naming the minimum runtime plan version and the sections it depends on. A runtime version that changes one of those sections lists the affected frontend requirement ids in its changes section. The frontend then publishes a new version if any of its requirements change, and updates only the `Requires` row in place if none do.
6. **Requirement ids.** Frontend requirements use `F-<AREA>-<nn>`, acceptance scenarios `FS-<nn>`, and open decisions `FD-<nn>`. An id is never renumbered or reused. A removed requirement moves to the plan's withdrawn table with the version that withdrew it. The runtime plan keeps its existing convention of section numbers, which archives preserve.
7. **Independent version sequences.** These are different numbers, and no document may treat one as another:

   | Sequence | Where it lives | Defined by |
   | --- | --- | --- |
   | Runtime plan version | `PLAN.md` header | This file |
   | Frontend plan version | `FRONTEND_PLAN.md` header | This file |
   | Artifact schemas | `schema` fields, ledger `PRAGMA user_version`, `schemas/*.json` | Runtime plan section 17 |
   | Protocol versions | Worker and legacy `schema`, desktop `api` version, helper version | Runtime plan sections 8 and 17 |
   | Package releases | `pyproject.toml` version, desktop application version | Release notes, which name the plan versions and artifact versions each release implements |

   The runtime plan's section 17 table maps each runtime plan version to the artifact and protocol versions it defines. A plan version bump does not imply a schema bump, and a schema bump always requires a plan version bump.
