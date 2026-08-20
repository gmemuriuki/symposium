# Agent interaction tests

Agent interaction tests exercise Symposium with scripted users, real processes, and selected real coding agents. They complement ordinary tests: use deterministic tests for exhaustive Symposium logic and these journeys when the process, terminal, user, or agent boundary is itself under test.

The feature is experimental. Real-agent runs consume provider capacity and are opt-in.

## Discover scenarios

```console
cargo xtask agent-test --list
```

The scenario list reports required agent, environment, operating-system, and witness capabilities. The RFD's contract table maps the tracer's Symposium promises to executable scenarios and linked product gaps.

Running `cargo xtask agent-test` without a scenario prints an execution plan and does not start an agent.

Repeat `--scenario` to select more than one journey:

```console
cargo xtask agent-test --agent claude --environment container --auth api-key --scenario dependency-consent-accept --scenario dependency-consent-decline
```

## Run on the host

```console
cargo xtask agent-test --agent claude --environment host --auth local --scenario dependency-consent-accept
```

The host runner creates fresh project, Symposium, agent, cache, and temporary directories. Local authentication is used only when explicitly requested. If the adapter cannot separate credentials from normal agent configuration, the result is marked non-authoritative.

Host runs are useful for debugging but may still be affected by installed tools and the operating system.

## Run Linux conformance

```console
$env:ANTHROPIC_API_KEY = "..."
cargo xtask agent-test --agent claude --environment container --auth api-key --scenario dependency-consent-accept
```

The runner prepares a pinned base image and one content-addressed Linux `cargo-agents` build from the checkout. Each scenario receives a fresh restricted container. Scenario runtime is hermetic except for the selected agent provider.

To test an existing compatible Linux artifact:

```console
cargo xtask agent-test --agent claude --environment container --auth api-key --symposium-bin ./artifacts/cargo-agents-linux-x86_64 --scenario dependency-consent-accept
```

The override is checked for operating system, architecture, executable format, and available version metadata. The runner never substitutes a released package, PATH binary, host environment, or different execution backend silently.

## Read the execution plan

Before a paid run, the plan reports information such as:

```text
Selected scenarios:       2
CLI-only scenarios:       1
Real-agent scenarios:     1
Maximum agent turns:      1
Maximum provider requests: 4
Maximum tool calls:       3
Input-side token guard:   25,000
Output-token guard:       1,000
Daily tokens remaining:   30,000
Monthly tokens remaining: 750,000
Per-run cost allowance:   $0.20
Monthly provider cap:     $5.00
Environment:              Linux container
Agent/runtime:            Claude, pinned
```

Real-agent scenarios enforce cumulative input, cache-read, cache-write, and output tokens as well as provider-request, turn, tool-call, deadline, and run-wide limits. Cached tokens still count even when they cost less.

The runner reserves the complete scenario ceiling from its daily and monthly ledgers before contacting the provider. If there is not enough capacity, the run does not start. A retry needs a separate reservation. The initial tracer permits at most one paid run per day and 25 per month. Its base-token estimate is approximately $0.09 per run, its allowance including cache-price differences is $0.20, and its dedicated provider key has a $5 monthly cap. Measured calibration usage lowers the scheduled token limit; it never rises automatically.

## Read a result

Each run ends as:

- `Passed`: the journey and assertions succeeded.
- `Failed`: the environment worked, but the behavior violated the contract.
- `InfrastructureError`: setup, authentication, provider, runtime, harness, or an operator-imposed budget stopped the run.
- `Unavailable`: the selected combination lacks a required capability.

Results may carry modifiers. `non-authoritative(contaminated-auth-context)` means local authentication could not be isolated from agent configuration. `stability-warning(recovered-infrastructure-error)` means a complete fresh-state retry recovered from a recognized infrastructure failure. A modifier cannot turn a product failure into a pass or satisfy a conformance requirement with non-authoritative evidence.

A scenario that cannot produce its witness within its own token budget is `Failed`. Oversized harness context, exhausted daily or monthly capacity, or a lower operator limit is `InfrastructureError` owned by `runner.budget`. Scheduled paid execution is `Unavailable` when the adapter cannot report trustworthy usage.

The summary also names the owning phase. Only a recognized transient infrastructure error may retry the entire scenario once with fresh state. Product failures and individual steps are never retried. A known-gap reproducer still returns `Failed` when run directly; scheduled reports identify it separately from release-gating covered scenarios.

Artifacts are under `target/agent-tests/<run-id>/`. Failure artifacts contain only allowlisted, sanitized evidence and a redaction report. Complete homes, authentication directories, and process environments are never archived. Use `--keep-artifacts` to retain rich evidence for a passing run.

## Write a scenario

Scenarios use typed Rust builders but contain portable data rather than arbitrary closures. Every behavioral branch is a separate linear scenario with fresh state.

A typical consent journey:

1. Compose a Rust fixture whose dependency embeds a plugin awaiting consent.
2. Start with empty Symposium and agent configuration.
3. Run real `cargo agents init --add-agent <agent>` and assert setup.
4. Run real `cargo agents sync` under a parsed PTY.
5. Select the intended prompt option with explicit keys.
6. Assert terminal anchors, structured events, exit status, and final state.
7. Start a persistent agent session when delivery is under test.
8. Assert a narrow capability witness such as a fixture nonce, hook trace, or MCP server log.

Scenarios declare contract IDs, required capabilities, permissions, scenario-owned token and operation budgets, and external endpoints. They do not contain Claude-specific paths or judge general response quality. An operator-supplied lower budget is shown separately and cannot manufacture a Symposium failure.

Time-dependent scenarios mutate controlled persisted inputs instead of sleeping or changing the production clock. They may set a cache expiry into the past, write a fixture `state.toml`, set a file mtime, or disable the sync debounce. Process and agent deadlines always use real monotonic time.

## CI operation

Deterministic tests block pull requests. Stable agent-free PTY and small Linux-container scenarios may graduate after meeting runtime and reliability criteria. Real-agent tracer journeys run in trusted scheduled or manual jobs and begin as non-gating observations.

### Future release gating

A real-agent journey can become release-gating after at least 19 of its latest 20 eligible scheduled runs pass for the same pinned manifest. Assertion failures are not retried. Falling below the target requires a reviewed quarantine decision with a linked issue; quarantined journeys continue to run and report.
