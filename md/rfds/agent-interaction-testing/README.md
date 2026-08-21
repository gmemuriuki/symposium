# Agent interaction testing

## TL;DR

- Extend the existing test infrastructure with scripted user, CLI, and real-agent journeys.
- Start every journey from controlled fixtures and isolated user and agent state.
- Run production-facing `cargo agents` processes under a real PTY.
- Keep exhaustive registry logic deterministic; use real agents only where delivery into an agent can fail.
- Use Claude first without placing Claude-specific concepts in scenarios.
- Use a Linux container for isolated Linux conformance and native lanes for Windows and macOS behavior.

## Motivation

Symposium's value comes from the interaction between discovery, consent, configuration, skills, hooks, MCP servers, caches, the user, and the coding agent. Existing integration tests cover much of Symposium's logic, but they do not consistently exercise the complete production boundary. For example, report events currently can reach a hook's stdout before its protocol payload, while an in-process dispatch assertion can still observe the correct event. The accepted `disable` precedence also disagrees with one current enablement path. Neither boundary failure is made obvious by the current integration suite.

We want to begin with fixture directories and files, run real commands as a user would, answer interactive prompts, start real agents, and inspect what happened. The harness must make failures reproducible and distinguish a Symposium contract violation from an unavailable runtime or provider failure.

This is integration testing, not agent evaluation. Measuring whether Symposium makes agents write better Rust requires baselines and statistical analysis and is outside this RFD. A future evaluation system may reuse these fixtures, adapters, and environments.

## Behavioral contract

The tests verify that Symposium:

- discovers extensions relevant to the current project;
- respects trust and explicit user choices;
- delivers enabled skills, hooks, MCP servers, and subcommands;
- expresses journeys through an adapter-neutral contract; and
- makes failures visible and contains their effects.

Accepted RFDs and reference documentation define expected behavior, even when the implementation currently disagrees. For example, the accepted registry contract says `disable` overrides `use` and `auto-enable`; one current code path does not yet enforce that rule consistently. The coverage table marks this as follow-on direction instead of copying the bug into the expected result or claiming coverage. It becomes `Gap(issue)` only when a linked issue and executable reproducer exist.

## First journey

The tracer journey starts with an empty Symposium home, empty agent configuration, and a Rust project whose dependency embeds a plugin awaiting consent.

```text
run cargo agents init --add-agent claude
run cargo agents sync under a PTY
wait for the dependency suggestion
choose Enable
assert the visible prompt, structured events, exit status, config, and files
run one bounded query through the Claude adapter
assert a fixture capability witness
```

A separate decline scenario begins from fresh state, selects “No, don't ask again,” restarts the CLI, and proves that the decision persists and the prompt does not return. Ask-later and Escape variants record nothing.

The same registered scenario runs through in-process, native-process, Linux-container, and selected real-agent layers. Unsupported combinations are reported explicitly rather than silently weakened.

## Design chapters

- [Scenario model](./scenario-model/README.md) defines declarative registration metadata, imperative Rust bodies, the production process boundary, PTY scripting, and explicit time-state fixtures.
- [Agent adapters](./agent-adapters/README.md) defines Claude, ACP and fake follow-ups, bounded queries, capability witnesses, permissions, and runtime pinning.
- [Execution environments](./environments/README.md) defines host and container isolation, native OS coverage, binary provenance, initialization, networking, trust, and authentication.
- [Evidence and results](./evidence/README.md) defines dual observation, canonical events, assertions, results, retries, cleanup, and artifact safety.
- [Coverage and CI](./coverage-and-ci/README.md) defines the contract table, coverage obligations, tracer journeys, command interface, cost controls, and implementation steps.
- [Proposed guide](./proposed-guide/README.md) shows how developers would list, run, inspect, and author scenarios.

## Key boundaries

The new engine is additive. Existing fixtures, `TestContext`, simulations, and deterministic tests remain. `cargo test` handles fast coverage; `cargo xtask agent-test` is a thin orchestration frontend for selecting expensive environments and agents.

Authoritative user journeys execute the compiled binary through `cargo agents`; they do not substitute an in-process call. PTY output proves that a user can see and answer a prompt, while a structured side channel and final state prove the underlying decision.

Scenario fixtures are reviewed repository content. “Untrusted” means awaiting user consent, not hostile code. The container improves reproducibility and least privilege but is not claimed as a sandbox for malicious extensions.

Every real-agent journey has a bounded capability witness. General prose and code quality are not graded. Claude is the first production adapter. Fake and ACP conformance are required follow-ups before the provisional driver interface can be called stable.

## Scope and milestones

This RFD is implemented when the tracer is proven: the consent journey works through real native processes, a parsed PTY, structured evidence, a fresh Linux container, and one bounded Claude capability witness, with useful failure artifacts and measured runtime.

Broader catalog automation, consent branches, cross-platform process lanes, fake and ACP conformance, hook and MCP witnesses, and trusted release CI are follow-on direction rather than acceptance criteria for this RFD. They require tracked issues or follow-on RFDs after the tracer informs the interfaces. See [Coverage and CI](./coverage-and-ci/README.md#milestones-and-follow-on-direction).

## Frequently asked questions

### Does this replace the current integration tests?

No. It reuses them and adds missing process, PTY, isolation, observation, and agent-delivery seams. A real-agent query is added only when activation inside the agent is the behavior under test.

### Why not adopt one of the linked CLI testing projects?

`cli-testing-library` provides a useful screen-query and user-event model, which this design borrows. Its Node implementation and platform constraints are not a good foundation for this Rust, cross-platform harness. `cli-testing-specialist` targets generic generated CLI validation and does not provide Symposium-specific state, consent, hooks, MCP, or agent-delivery behavior.

## Implementation status

This RFD describes proposed experimental infrastructure. Implementation has not begun, and the tracer milestone has not been reached.
