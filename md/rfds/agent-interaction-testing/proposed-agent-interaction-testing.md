# Agent interaction tests

Agent interaction tests exercise Symposium together with a real coding agent in a controlled project. They complement the ordinary test suite: use ordinary tests for exhaustive CLI and registry logic, and use these journeys when the interaction among the user, Symposium, and the agent is the behavior under test.

The feature is experimental. Real-agent runs consume provider capacity and may expose provider variability, so they are opt-in and separate from `cargo test`.

## Discovering scenarios

List the scenarios and their required capabilities:

```console
cargo xtask agent-test --list
```

The output identifies whether the selected agent and environment can run each scenario. A missing capability is reported as unavailable rather than being mistaken for a product failure.

## Running locally

For a fast development run, use the host environment:

```console
cargo xtask agent-test --agent claude --environment host --scenario trusted-dependency-guidance
```

The host runner creates isolated project, home, configuration, cache, and temporary directories. It may use the agent authentication already present on the machine. Host results are useful for debugging but are not authoritative because installed tools and the operating system can still influence the run.

## Running a production-conformance journey

Use the container environment for the authoritative Linux rehearsal:

```console
$env:ANTHROPIC_API_KEY = "..."
cargo xtask agent-test --agent claude --environment container --scenario trusted-dependency-guidance
```

The runner prepares a cached base image and a thin layer containing the Symposium binary for the current revision. It then creates a fresh, restricted container for the scenario. All turns and deliberate restarts in that scenario share its state.

To test an already-built compatible Linux artifact, select it explicitly:

```console
cargo xtask agent-test --agent claude --environment container --symposium-bin ./artifacts/symposium-linux-x86_64 --scenario trusted-dependency-guidance
```

The runner never silently substitutes a released package or falls back from a requested container to the host.

## Reading a result

Each run ends as one of:

* `Passed` — the journey and its graders succeeded;
* `Failed` — the environment worked, but observed behavior violated the scenario;
* `InfrastructureError` — setup, authentication, provider access, or the runner failed;
* `Unavailable` — a requested driver or environment lacks a required capability.

The console summary names the failed step and points to `target/agent-tests/<run-id>/`. Failure artifacts include a sanitized event journal, relevant logs, the conversation transcript, file diffs, and final inspected state. Use `--keep-artifacts` to retain the same detail after a successful run:

```console
cargo xtask agent-test --agent claude --environment container --scenario consent-enable --keep-artifacts
```

## Writing a scenario

Scenarios are initially written with typed Rust builders, but contain portable data rather than arbitrary closures. A typical scenario describes this sequence:

1. Create a Rust fixture whose dependency has a trusted Symposium extension.
2. Start a persistent agent session in the fixture.
3. Ask the agent to implement a small, controlled Rust task.
4. Wait for protocol completion rather than sleeping for a guessed duration.
5. Assert that Symposium selected and delivered the extension.
6. Inspect the resulting files and run targeted Rust checks.

Check exact state at deterministic boundaries. For example, check that consent was recorded, a hook event occurred, an MCP tool was invoked, or `cargo test` passed. Do not snapshot an entire model answer or require incidental wording.

A scenario declares capabilities such as `persistent-conversation`, `pty-input`, `hooks`, and `mcp`. It does not contain Claude-specific branching. Agent-specific behavior belongs in the adapter.

## CI operation

Container journeys run only in a trusted scheduled or manually dispatched workflow. The workflow uses a restricted spending credential and does not expose it to fork pull requests. While the suite is experimental, failures are visible but do not block ordinary pull requests.

When investigating runtime, compare the recorded phases separately: image preparation, environment startup, agent execution, and grading. A slow provider turn should not be diagnosed as slow container startup.
