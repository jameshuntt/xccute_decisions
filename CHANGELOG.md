# Changelog

## 0.2.1 (2026-10-19)

the first release since 0.1.1, and everything since it. 0.2.0 was tagged in git (`xccute_decisions-v0.2.0`, 6d8df4a) and never published; its source is the same here, and the license notice and the README moved on after the tag, so this release takes the next number instead of reusing the tag's. xccute 0.3.1 depends on it.

### the connector names a public crate can carry

- the two connector constructors were named for the pilot's daemons, `nodeplan` and `denv`, with `nodeplan`/`nodeplan-control` and `denv`/`denv-control` as their kinds: [src/runbook.rs L98-L129](https://github.com/jameshuntt/xccute_decisions/blob/f3b9baf09080d3cc82bd3cd00c49de9c21af8d5c/src/runbook.rs#L98-L129). they are `supervisor` (a runbook request from a fleet supervisor) and `environment` (from an environment daemon), with `supervisor`/`supervisor-control` and `environment`/`environment-control`: [src/runbook.rs L97-L131](https://github.com/jameshuntt/xccute_decisions/blob/d4e8790eccfb598653887f7ad15e0d5b424bf841/src/runbook.rs#L97-L131). this is the one change to the public API: `nodeplan()` and `denv()` are gone, renamed, not added beside; it was made at 0.2.0 and is kept as it was made.
- the crate doc says supervisor-style connectors instead of NodePlan/denv-style: [src/lib.rs L7-L7](https://github.com/jameshuntt/xccute_decisions/blob/d4e8790eccfb598653887f7ad15e0d5b424bf841/src/lib.rs#L7-L7). the tests use fleet names for their plans and ids: every test file under tests/.

### Cargo, license, CI

- xccute_runtime 0.1.1 (the verified operation and plan transition seams the runbook records): [Cargo.toml L16-L16](https://github.com/jameshuntt/xccute_decisions/blob/d4e8790eccfb598653887f7ad15e0d5b424bf841/Cargo.toml#L16-L16).
- the Apache notice names 1o LLC: [LICENSE-APACHE L190-L190](https://github.com/jameshuntt/xccute_decisions/blob/d4e8790eccfb598653887f7ad15e0d5b424bf841/LICENSE-APACHE#L190-L190).
- CI publishes from main on a version bump through crates.io trusted publishing (registered for jameshuntt/xccute_decisions, ci.yml), after the tests on stable and 1.70 and clippy: [.github/workflows/ci.yml](https://github.com/jameshuntt/xccute_decisions/blob/d4e8790eccfb598653887f7ad15e0d5b424bf841/.github/workflows/ci.yml); the README's Releasing says so: [README.md L14-L17](https://github.com/jameshuntt/xccute_decisions/blob/d4e8790eccfb598653887f7ad15e0d5b424bf841/README.md#L14-L17). CHANGELOG.md is in the package: [Cargo.toml L13-L13](https://github.com/jameshuntt/xccute_decisions/blob/d4e8790eccfb598653887f7ad15e0d5b424bf841/Cargo.toml#L13-L13).
- version 0.2.0 to 0.2.1: [Cargo.toml L3-L3](https://github.com/jameshuntt/xccute_decisions/blob/d4e8790eccfb598653887f7ad15e0d5b424bf841/Cargo.toml#L3-L3).

### proof

- `cargo test`: 39 passed (stable, and 1.70 on the lock CI makes for it).
- public API against 0.1.1 (rustdoc JSON): `supervisor` and `environment` added, `nodeplan` and `denv` removed, nothing else changed.
