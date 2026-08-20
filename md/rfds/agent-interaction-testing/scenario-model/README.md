# Scenario model

## Shared engine

Agent interaction tests extend `symposium-testlib`; they do not create a second fixture or assertion system. Ordinary tests and `cargo xtask agent-test` use the same fixture composition, scenario model, event vocabulary, and assertions.

`cargo test` remains the frontend for deterministic and selected host scenarios. Xtask is a thin orchestration frontend for environment selection, credentials, containers, filtering, real-agent execution, and artifact retention.

## Scenario data

Scenarios are authored with typed Rust builders. The underlying model contains portable data rather than arbitrary closures, so it can be serialized and may later gain a TOML or YAML frontend.

A scenario declares:

- fixture layers and controlled services;
- required environment and agent capabilities;
- ordered CLI, user, agent, mutation, restart, and checkpoint steps;
- a capability witness for every real-agent interaction;
- deadlines, resource limits, and scenario-owned agent budgets;
- a least-privilege permission policy;
- contract rule identifiers; and
- assertions and artifact-retention policy.

Scenarios select capabilities, not agent brands. Agent-specific paths, authentication fields, event types, and witness mechanisms remain in adapters.

Agent token budgets are cumulative across every provider request made for the journey, not merely the number of user-visible turns. They bound input, cache-read, cache-write, and output tokens separately, plus provider requests and tool calls. Cached tokens still count toward the token budget even when their dollar price is lower.

## State and branching

Scenarios are linear. Accept, decline, ask-later, and Escape are separate scenarios that may reuse fixture descriptions but never writable state.

Every scenario and retry begins with a fresh workspace, user configuration, agent configuration, cache, services, and conversation. Steps within one scenario share state deliberately, including across declared process or agent restarts. Persistence and cache scenarios express repeated commands in that one journey because preserved state is what they test.

Scenario logic does not branch around unexpected output. A missing or different checkpoint fails at that step.

## Production process boundary

Authoritative user journeys invoke the compiled `cargo-agents` executable through the production-facing `cargo agents` command. Interactive commands run under a PTY. The runner captures the rendered terminal, sanitized raw bytes, exit status, structured events, and resulting state.

The runner must not silently replace a requested process or PTY step with an in-process call. Existing deterministic tests may continue calling Symposium's Rust entry points directly, but that path does not prove Cargo dispatch, PATH setup, terminal interaction, hook subprocesses, or process exit behavior.

## PTY scripting

The PTY driver parses ANSI output into a rendered screen instead of treating the stream as plain stdout. Waits query narrow anchors in that screen. Input steps represent lines and explicit keys such as Enter, Escape, arrows, EOF, and interrupt.

Ordinary scenarios use a fixed terminal size, UTF-8 locale, declared TERM and color mode, and the native PTY backend for the operating system, including ConPTY or equivalent on Windows. The terminal profile and backend are recorded with the result.

Screen normalization handles cursor movement, redraws, color, and newline differences. Raw sanitized bytes remain diagnostic evidence. A small rendering suite separately tests color and resizing; ordinary journeys do not snapshot the complete screen.

This adopts the useful interaction model from `cli-testing-library`: wait for what a user can see, then send user input, without adopting its Node implementation.

## Time-dependent scenarios

Tests never synchronize with fixed sleeps. Every wait targets an observable condition and has a real monotonic deadline.

This RFD does not add a production clock seam. Time-dependent tests mutate controlled persisted inputs before starting the command that observes them. Predicate-cache tests set the persisted expiry into the past, update-throttle tests set `state.toml`, filesystem tests set the relevant mtime, and sync tests may use `sync-debounce-secs = 0`.

These mutations test the production comparison against the real wall clock without waiting for time to pass. If a later contract cannot be tested this way, its clock abstraction requires a separate design. Container, agent, TLS, provider, and process deadlines always use real time.

## Why typed Rust instead of a scenario DSL?

Typed builders provide compiler-assisted refactoring, IDE discovery, and direct reuse of test helpers while the vocabulary is evolving. The tradeoff is recompilation and a higher contribution barrier for non-Rust authors. Keeping the model serializable and closure-free preserves the option to add a data-file frontend after the vocabulary stabilizes.
