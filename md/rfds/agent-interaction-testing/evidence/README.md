# Evidence and results

## Dual observation

Interactive journeys observe two channels from the same process:

- The normal PTY proves that a suggestion or prompt was visible and accepted real user input.
- A harness-controlled JSONL side channel reports stable Symposium decisions and state transitions.

The side channel attaches an additional reporting sink. It must not enable quiet mode, bypass confirmation, change command decisions, or write into a hook's protocol stdout. Events needed by the initial scenarios include discovery, confirmation requested and answered, enablement, installation, and hook dispatch. Later scenarios may add predicate and cache events without changing the product-facing stream.

Terminal assertions use only the stable text necessary to prove that a user could understand and answer the prompt. Colors, wrapping, and complete screens are not ordinary snapshots. Detailed behavioral assertions use structured events and final state.

## Canonical event journal

The runner merges harness events, Symposium side-channel events, agent protocol events, and observed state changes into one coarse journal while retaining sanitized source artifacts.

Each event envelope contains:

- schema version;
- run, scenario, and attempt identifiers;
- source and source-local sequence;
- correlated operation identifier;
- event kind;
- real monotonic offset; and
- normalized payload.

Provider-operation events record requested limits and reported input, cache-read, cache-write, and output tokens. Aggregate token counts and derived cost are evidence, not estimates substituted for missing accounting.

Sequence is strict within one source. Receipt order is diagnostic and does not imply causal order across processes. Assertions express partial order within a source or correlated operation, such as discovery before confirmation and confirmation before installation. Unrelated sources remain unordered unless explicitly correlated.

Dynamic paths, process IDs, ports, and timestamps are normalized before comparison. Unknown additive event kinds are retained and ignored unless required. Breaking envelope changes increment the schema version. Event payloads use stable identifiers and exclude secrets.

## Assertions

Authoritative assertions prefer:

- exact configuration and allowed filesystem state;
- process exit status and normalized system events;
- protocol completion and tool activity;
- hook stdout containing only the selected agent's protocol output;
- discovery, consent, predicate, and cache decisions; and
- hook, skill, MCP, and subcommand capability witnesses.

Model prose is checked only through a narrow fixture-defined nonce or fact when that is the available witness. Full responses are diagnostic, not gating.

## Results and failure ownership

A run has four results:

- `Passed`: the requested journey and assertions completed.
- `Failed`: the environment ran, but Symposium or the interaction violated the contract.
- `InfrastructureError`: credentials, provider, runtime, environment, or harness failed.
- `Unavailable`: preflight found that the selected adapter or environment lacks a required capability.

A result may also carry modifiers that preserve important qualifications without creating another base result:

- `non-authoritative(contaminated-auth-context)` means local credentials could not be separated from user or agent configuration.
- `stability-warning(recovered-infrastructure-error)` means a recognized infrastructure failure occurred before the complete fresh-state retry passed.

Modifiers are recorded in the summary, journal, and aggregate reports. They never turn `Failed` into `Passed` or make a non-authoritative run satisfy a conformance or release requirement.

Explicitly requesting an unavailable combination exits unsuccessfully; ordinary `cargo test` remains unaffected. There is no expected-failure scenario result. A known product-gap reproducer still returns `Failed` when run directly.

Suite aggregation consults the coverage table separately. A `Gap(issue)` reproducer runs on its reporting schedule but is excluded from the release gate and listed as a known gap. Adding a gap, removing its reproducer, or increasing the gap count requires review. Covered scenarios retain their ordinary gating behavior.

Failures name an owning phase such as `environment.prepare`, `symposium.cli`, `symposium.state`, `agent.start`, `agent.turn`, `fixture.mcp`, `assertion`, or `cleanup`. A Symposium crash, missing prompt, wrong state, or completed agent turn without its required witness is `Failed`.

Exceeding a scenario-owned token or operation limit is `Failed` because the witness did not fit its contract. Harness-controlled context that already exceeds the declared limit, exhaustion of a daily or monthly token ledger, or an operator ceiling stopping the run is `InfrastructureError` owned by `runner.budget`. Missing trustworthy provider accounting makes scheduled paid execution `Unavailable`.

## Retries and cleanup

Assertion and Symposium failures are never retried automatically. A recognized transient infrastructure error may retry the complete scenario once with fresh state. Individual steps are never replayed inside an existing container or conversation.

Both attempts are preserved. A recovered run remains `Passed` with the `stability-warning(recovered-infrastructure-error)` modifier and attempt metadata, so infrastructure reliability still counts the transient.

On a deadline, the runner captures current evidence, attempts graceful termination, kills the complete process tree after a bounded cleanup deadline, and verifies that no process or container remains.

## Artifact safety

Artifacts live under `target/agent-tests/<run-id>/`. Every run keeps a compact summary. Failures keep sanitized journals, terminal output, selected logs, workspace diffs, agent events, explicitly allowed Symposium state, and a redaction report. Successful runs keep rich artifacts only with `--keep-artifacts`.

Capture is allowlist-based. The runner never archives a complete container, home, authentication directory, or process environment. Credentials are secret handles supplied only to the process that needs them. Known values, provider headers, credential-bearing URLs, command arguments, and environment fields are redacted in memory before persistence.

Every run injects harmless secret canaries and verifies that none survive. If sanitization cannot complete, rich artifacts are withheld and the summary reports the redaction failure. Upload-time filtering is not considered sufficient.
