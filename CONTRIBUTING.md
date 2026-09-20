# Contributing

Thanks for helping build Heretek. This project is local-first: everything you
add must work without a cloud account.

## Before you start

- Read `docs/spec/0001-heretek-architecture.md` and the ADRs in `docs/adr/`.
- Check open issues on `Heretek-AI/Heretek`; one issue per tracer bullet.
- For a new architectural decision, open an ADR pull request first.

## Build and verify

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p heretek-cli -- --help
```

All three must pass before a commit.

## Conventions

- Rust 2024, workspace members under `crates/`.
- No comments unless they explain a non-obvious constraint.
- No language model calls inside `heretek-gate`. Gates are deterministic.
- Network use is limited to dependency/model downloads and OSV lookups.
- Gate diagnostics use the `heretek.diagnostic/1` frame; do not invent fields
  without updating the spec.

## Pull requests

- One concern per pull request; reference the issue number.
- Include a short "how verified" section: the commands you ran and what you
  observed. Screenshots of terminal output are fine.
- New stages or surfaces must state which spec section they implement.
- Keep `cargo clippy` clean; CI enforces it.

## Testing policy

The project currently prioritizes shipped surfaces over unit-test breadth.
Deterministic core logic (decide heuristics, pipeline semantics, report
shapes) keeps its tests, and the evaluation harness (`eval/`) is the main
evidence for agent behavior. When adding a blocking gate or a routing rule,
add at least one fixture-based test.
