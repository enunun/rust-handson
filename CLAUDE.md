# ferrodb hands-on

A hands-on course (in Japanese) where learners build `ferrodb`, a PostgreSQL-wire-compatible relational database, in Rust over 23 Iterations.
The course is built with the `system-development-skills:build-handson` skill. Read `COURSE.md` (decisions, conventions, pitfalls) and `docs/ROADMAP.md` (what each Iteration builds) before changing any course material.

# RTK (Rust Token Killer)

Prefix every shell command with `rtk`, including each command in an `&&` chain — it is always safe (a dedicated filter cuts noisy output for tests, builds, git, and more; anything without one passes through unchanged). The full command reference is in the global `~/.claude/RTK.md` (already loaded, if set up). Meta commands: `rtk gain` (savings so far), `rtk discover` (missed opportunities in past sessions), `rtk proxy <cmd>` (run unfiltered, for debugging).

## Working conventions

- Build one Iteration per run, in ascending order. Changes to features, topics or design documents go into `docs/ROADMAP.md` or `COURSE.md` first, agreed with the user.
- Every output shown in the material (compiler messages, test failures, REPL and psql sessions) is copied from a real run.
- `git commit` runs the lefthook hooks. If they fail, fix the reported issues. Do not use `--no-verify`.

- Run `mise run check` after making changes.

## Code map

- `iterations/iteration-NN/solution/`: model answers; Cargo workspace members (`ferrodb-NN-solution`, lib name `ferrodb`).
- `iterations/iteration-NN/exercise/`: learner packages (`ferrodb`), standalone and listed one by one in the root `Cargo.toml` `exclude`.
- `docs/`: roadmap, TDD and design guides, per-Iteration notes (`docs/rust/`, `docs/db/`).
- `scripts/`: Mermaid syntax check, design-to-code check, exercise tests.

# Artifact Cleanup

## Golden Rule

**Whenever you produce an artifact, always run the `system-development-skills:finalize-artifacts` skill to clean it up before reporting the work as done.**

An artifact is any deliverable you create or substantially rewrite: documents, READMEs, code and code comments, config files, scripts, commit messages, PR descriptions, and so on.

- Invoke the skill via the Skill tool (`system-development-skills:finalize-artifacts`) after the artifact is written and before the final reply.
- The skill edits the artifact files in place. Do not append a changelog of the cleanup to the artifact; in the final reply, mention what changed in a sentence or two at most unless the user asks for a full report.
- Skip it only for replies that produce no artifact (answering questions, explaining code, running read-only commands).
- Provided by the `enunun/system-development-skills` plugin (see `extraKnownMarketplaces`/`enabledPlugins` in `.claude/settings.json`).
