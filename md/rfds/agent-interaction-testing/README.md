# Agent interaction testing

## TL;DR

- Add an experimental integration-test suite that drives real coding agents through realistic, multi-turn user journeys.
- Keep the existing deterministic integration tests; the new suite tests the combined behavior of Symposium and an agent.
- Define scenarios independently of any agent, with Claude as the first adapter.
- Support a fast host runner for development and an authoritative Linux-container runner for production conformance.
- Record exact system evidence and use narrow semantic checkpoints instead of matching entire model responses.

## Motivation

Symposium aims to be a one-stop shop that helps coding agents write great Rust code. Its value does not come from one isolated command. It comes from the interaction between discovery, user consent, skills, hooks, MCP servers, configuration, caching, and the coding agent itself.

The existing integration tests are valuable for deterministic CLI and registry behavior. They cannot answer the larger question: given a realistic Rust task, does a real agent receive and use what Symposium provides, and does the resulting interaction behave as intended?

We need a production rehearsal that starts from a controlled fixture, isolates agent and user state from the developer's machine, drives a realistic conversation, and captures enough evidence to explain both success and failure.

## Product contract

The suite distinguishes guarantees from hypotheses.

Symposium's platform guarantee is that it:

- discovers extensions relevant to the current project;
- respects trust and explicit user choices;
- delivers enabled skills, hooks, and MCP servers to supported agents;
- behaves consistently across agent adapters where their capabilities overlap; and
- makes failures visible and contains their effects.

Our initial outcome hypothesis is that this platform helps an agent produce more correct, idiomatic, and current Rust. Version one checks representative journeys and concrete outcomes. It is not a statistical model benchmark and does not claim that every successful run proves a general improvement over a baseline agent.

The first audience is an individual Rust developer using an agent in a repository. Organization policy, team-managed configuration, and crate-author publishing journeys are valuable later extensions, but they are not required to validate the first design.

## Change in a nutshell

Add a separate `cargo xtask agent-test` suite built around four replaceable boundaries:

1. a serializable, agent-neutral scenario description;
2. an agent adapter that exposes capabilities and a persistent conversation;
3. an environment backend that runs either on the host or in an isolated container; and
4. graders that inspect exact state, normalized events, and bounded semantic outcomes.

The suite is additive. Existing fixtures, `TestContext`, simulations, and deterministic tests remain the primary way to cover the combinatorial corners of the registry and CLI. Real-agent journeys cover a smaller set of representative end-to-end interactions.

## Detailed plans

### Scenario model

Scenarios are authored with typed Rust builders at first. The underlying model remains pure data: steps cannot contain arbitrary Rust closures. This keeps scenarios serializable and leaves open a later YAML or TOML representation without committing to a DSL before the vocabulary is understood.

A scenario contains:

- a fixture describing project files, registry data, and local services;
- required capabilities such as persistent conversation, PTY input, hooks, or MCP;
- environment and authentication requirements;
- ordered user, CLI, mutation, restart, and checkpoint steps;
- deadlines and resource limits; and
- graders and artifact-retention policy.

There are two interaction channels:

- PTY steps drive an interactive CLI by waiting for an observable prompt or state, sending a line or key, and checking exit status.
- Agent turns send a user message to a persistent structured agent session and wait for a protocol completion event.

Tests never synchronize with fixed sleeps. Every wait targets an observable condition and has a deadline.

### Agent-neutral adapters

An `AgentDriver` reports its capabilities, prepares an isolated runtime, starts and stops a persistent session, sends turns, and returns both normalized events and raw provider artifacts.

Scenarios select capabilities, not brand names. If the selected driver or environment cannot provide a required capability, the result is `Unavailable`, not a misleading test failure.

Claude is the first adapter because it is already used by Symposium developers. Its structured SDK is used for the main journeys so that completion and tool activity are observable. One narrow PTY smoke test covers the real interactive Claude entry point. Claude-specific protocol details must remain inside the adapter.

The existing `AgentSession::ClaudeSdk` path starts a new provider query for every prompt. The new adapter must maintain one conversation across turns so that confirmation, follow-up work, and later-session behavior are genuine interactions.

### Environment backends

The host backend is optimized for fast local iteration. It creates fresh project, home, configuration, cache, and temporary directories; filters inherited environment variables; and may reuse the developer's local agent authentication. It is useful but not authoritative because the host OS and installed tools can still affect results.

The container backend is the production-conformance environment. Version one uses Linux containers through Docker, behind an environment abstraction that can later support another container runtime, a VM, or a remote worker.

Each scenario receives a fresh container, while all turns and deliberate agent restarts within that scenario share its writable state. The fixture is copied into the container rather than mounting the repository. The container runs as a non-root user with a read-only root filesystem, dropped capabilities, no Docker socket, explicit writable directories, and CPU, memory, process, and time limits.

The container image is layered and cached. A stable base contains the agent runtime and ordinary tools. The current compatible Symposium Linux binary is built once per revision, or supplied explicitly with `--symposium-bin`, and copied into a thin test layer. We do not install Symposium from a package manager: doing so would test a released artifact rather than the code under development and would make the suite depend on registry and network speed.

This also avoids copying a binary during every scenario. Image preparation is incremental; scenarios start only after the revision-specific layer exists.

### Network and authentication

Authoritative container runs use a restricted CI API key. Host runs may reuse local Claude authentication for developer convenience.

The scenario container has no direct external egress. Provider traffic passes through a controlled forward-proxy sidecar connected to both an internal scenario network and an egress network. The proxy permits only provider endpoints and any endpoints explicitly declared by the scenario. Fixture services and local MCP servers stay on the internal network.

The CI credential is available only to trusted scheduled or manually dispatched jobs, never to fork pull requests. Version one is experimental and non-blocking while cost, stability, and diagnostic quality are measured.

### Evidence and grading

The runner writes a canonical, coarse event journal while retaining raw agent and process artifacts. The canonical vocabulary includes events such as process start and exit, prompt observed, input sent, agent turn completed, tool invoked, file changed, configuration changed, and grader completed.

Authoritative assertions prefer:

- exact files and configuration state;
- exact process status and normalized system events;
- protocol-level completion and tool activity; and
- task-specific graders such as compilation, tests, or targeted source inspection.

Model prose is checked only through narrow, stable semantic anchors when it is itself part of the contract. Full-response snapshots and exact wording are diagnostic, not gating.

A run has one of four results:

- `Passed`: the requested journey completed and all graders passed;
- `Failed`: the environment ran correctly but the behavior violated an assertion;
- `InfrastructureError`: setup, credentials, provider access, or the runner failed;
- `Unavailable`: the selected agent or environment lacks a required capability.

An explicitly requested unavailable run exits unsuccessfully and explains the missing capability. Ordinary `cargo test` is unaffected because this suite is opt-in.

Artifacts live under `target/agent-tests/<run-id>/`. Every run keeps a compact summary. Failures keep sanitized journals, transcripts, logs, diffs, and relevant final state. Passing runs retain full artifacts only with `--keep-artifacts`. Secrets must be removed before artifacts are persisted.

### Initial journeys

The first suite should contain a few high-value vertical journeys:

1. A fresh agent enters a Rust fixture with a trusted dependency, receives the relevant Symposium guidance, completes a controlled task, and passes Rust-specific graders.
2. An untrusted dependency triggers consent. Separate variants enable and decline it, and a later session honors the recorded choice.
3. A hook affects agent behavior and leaves the expected observable evidence.
4. An MCP-assisted task demonstrates that the configured server is available and useful to the agent.
5. A registry resynchronization is observed, followed by one deliberately broken extension whose failure is visible and contained.

Combinatorial cases such as every predicate permutation, cache boundary, or malformed registry entry remain in deterministic tests. A real-agent journey is added when the interaction between user, Symposium, and agent is what could fail.

### Command-line interface

The proposed entry point is:

```console
cargo xtask agent-test [OPTIONS]
```

Initial options are:

```text
--list
--agent <agent>
--environment <host|container>
--scenario <name>
--symposium-bin <path>
--keep-artifacts
```

Running container scenarios without a working runtime, compatible binary, or required credential produces an explicit `Unavailable` result. It must never silently fall back to the host backend.

### Time and cost

Containers add image preparation and startup time, but real-agent latency will usually dominate. The runner records cold image preparation, warm environment startup, agent time, and grading time separately so that optimization is based on measurements.

Fast deterministic tests continue to run on every change. Host journeys are for focused development. A small container suite runs on a trusted schedule or manual dispatch. This layering avoids multiplying expensive agent calls across the full deterministic matrix.

## Frequently asked questions



### Does this replace the current integration tests?

No. The current tests give faster, deterministic, exhaustive coverage of Symposium's own logic. The new suite adds evidence about real interactions. Existing infrastructure should only change where a small reusable seam makes both suites clearer. The old one-shot real-agent path may be removed after the persistent adapter supersedes it.

### Why typed Rust scenarios instead of YAML or TOML?

Typed builders provide compiler-assisted refactoring, good IDE discovery, and direct reuse of test helpers while the scenario vocabulary is still changing. The cost is that non-Rust contributors cannot edit a data file and scenarios must be recompiled. Keeping the scenario model serializable and closure-free preserves an escape hatch: once the vocabulary stabilizes, a data format can be added as another frontend.

### Can deterministic conversation checkpoints work with a nondeterministic model?

Yes, if the checkpoints target deterministic boundaries. We can exactly check which configuration changed, whether a prompt was answered, which capability became available, what files were produced, and whether the Rust task passes. We should not require the model to emit an exact paragraph.

### Why not copy the developer's installed Symposium executable into every test?

The installed executable may not match the checkout and copying it per scenario wastes time. The default authoritative path builds or accepts one compatible Linux binary and caches it in a thin image layer. An explicit `--symposium-bin` remains useful for testing a known artifact.

### Why are containers not the only backend?

The host backend shortens the edit-test-debug loop and can use local authentication. The container backend answers the stronger production-conformance question. Treating them as implementations of one environment interface keeps scenarios portable without pretending that host isolation is complete.

### What do the linked CLI testing projects contribute?

`cli-testing-library` demonstrates a useful interaction model: wait for observable output, query the screen, send user events, and avoid hand-written timing. Its Node implementation and reported platform constraints make it a reference rather than a foundation for this Rust, cross-platform suite.

`cli-testing-specialist` is oriented toward generic, generated CLI validation. Our journeys need persistent agents, Symposium-specific state, hooks, MCP, consent, and outcome graders, so adopting it would not remove the hard integration work.

### How will we know whether Symposium caused an idiomatic Rust result?

Version one proves delivery and checks bounded task outcomes: the relevant extension was selected, the agent could use it, and the fixture satisfies concrete Rust graders. Strong causal claims require repeated paired runs against a no-Symposium baseline and statistical analysis. That is a future evaluation layer, not a prerequisite for integration testing.

## Implementation plan

1. Introduce the scenario, capability, event, artifact, and result types with fake drivers and runner tests.
2. Add the host backend and a persistent Claude adapter.
3. Add the PTY driver for interactive Symposium and one Claude entry-point smoke test.
4. Add the Linux container backend, revision-layered Symposium binary, proxy, isolation rules, and infrastructure diagnostics.
5. Implement the trusted-dependency guidance journey as the first production rehearsal.
6. Add consent, hook, MCP, resynchronization, and contained-failure journeys.
7. Add the trusted scheduled/manual CI workflow, document operation and cost, correct stale design documentation, and retire superseded one-shot agent-test code.



## Implementation status

This RFD describes proposed experimental infrastructure. Implementation has not begun.

See [Proposed: Agent interaction tests](./proposed-agent-interaction-testing.md) for the intended operator workflow.