# xccute_decisions

The decision vocabulary of [xccute](https://crates.io/crates/xccute): what a
process asks before it acts, and how the answer is recorded.

- `ContextRequest` and `ContextPack`: the questions and the evidence gathered.
- Observation tools: read-only commands (`stat`, `find`, `ps`, `pgrep`) as
  the things that answer a question.
- `DecisionGuide` templates and `DecisionPath`s with optional and required steps.
- Runbooks, runbook records and the journal that keeps them.

Nothing here executes; execution is [`xccute_runtime`](https://crates.io/crates/xccute_runtime).

## License

MIT OR Apache-2.0.
