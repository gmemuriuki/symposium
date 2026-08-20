# Coverage and CI

## Contract table

The tracer begins with a reviewed Markdown table of the Symposium promises it exercises. Each row has a stable rule identifier, a behavioral statement, a link to the accepted specification, its required test layers, and one state:

- `Committed(step)`: this RFD commits to implementing the row in the named tracer step. It becomes `Covered` after the required scenarios pass.
- `Covered`: every required tracer scenario exists and passes.
- `Gap(issue)`: the implementation is known to violate the specification, a linked issue owns the discrepancy, and an executable reproducer returns `Failed` when run directly. The reproducer reports on its schedule but is excluded from the release gate.
- `Direction(follow-up)`: the rule is outside this RFD's tracer commitment and must be carried into a closing follow-up issue or RFD. It is not counted as tracer coverage.

Accepted RFDs and current reference documentation remain authoritative. The table must not copy an implementation bug into the expected result. Adding a gap, removing its reproducer, or increasing the gap count requires review.

Typed Rust scenarios name the rules they prove. After enough journeys exist to expose stable catalog requirements, a follow-up may make the table machine-readable, validate layer and operating-system obligations, and generate a coverage report. This RFD does not build that meta-tool before the first journey.

## Coverage layers

Every `Covered` contract rule has deterministic coverage. Covered user-visible branches have real-process scenarios. A PTY is required only when the production command is interactive; hooks and other noninteractive subprocesses use piped stdin, stdout, and stderr. Covered agent-delivery mechanisms have selected real-agent witnesses. The matrix below records intended obligations, including follow-on direction; it does not claim those rows are implemented by the tracer. Linux containers rehearse representative production paths rather than duplicating the deterministic matrix.

| Rule ID | Contract | State | Deterministic | Real process | Real agent |
|---|---|---|---:|---:|---:|
| `consent.accept` | Undecided candidate is accepted | `Committed(steps 1, 4)` | required | required, PTY | one delivery smoke |
| `consent.decline` | Undecided candidate is declined | `Committed(step 1)` | required | required, PTY | not required |
| `consent.defer` | Ask later records nothing | `Direction(follow-up)` | required | required, PTY | not required |
| `cli.noninteractive` | Noninteractive execution never prompts | `Direction(follow-up)` | required | required, pipes | not required |
| `enablement.disable-precedence` | Disable overrides other enablement | `Direction(follow-up)` | required | representative, pipes | not required |
| `cache.expiration` | Cache expiration reevaluates its input | `Direction(follow-up)` | required | required, pipes | not required |
| `hook.stdout-protocol` | Hook stdout contains only protocol output | `Committed(step 1)` | required | required, pipes | not required |
| `isolation.skill-inventory` | Isolated custom-skill inventory exactly matches the fixture | `Committed(steps 1, 3)` | required | required, pipes | required |
| `use.search-endpoint` | Non-workspace `use` search uses only its declared fixture endpoint | `Direction(follow-up)` | required | required, pipes | not required |
| `delivery.hook` | Hook delivery reaches an agent | `Direction(follow-up)` | required | representative, pipes | required |
| `delivery.mcp` | MCP registration reaches an agent | `Direction(follow-up)` | required | representative, pipes | required |

Operating-system applicability is recorded separately. A Linux-container pass cannot satisfy a Windows-native or macOS-native requirement.

The first audit must include conflicting enablement entries. The accepted registry contract says `disable` wins over `use` and `auto-enable`, even though one current implementation path checks `use` first.

## Tracer journeys

This RFD commits to two fresh-state consent journeys. The accepted branch runs real `init` and `sync`, answers the prompt, verifies installation, and obtains a capability witness from a persistent Claude session. The declined branch verifies that nothing is installed, the decision persists, and a later sync does not ask again.

The tracer also adds the deterministic hook-stdout regression because the structured side channel must not contaminate an agent protocol. It plants an isolation canary and verifies the exact custom-skill inventory owned by the fixture.

Ask-later and Escape, custom predicates, cache reuse and expiration, malformed registries, enablement precedence, non-workspace `use`, hook delivery, MCP delivery, and registry resynchronization remain stated follow-on families. They use deterministic or real-process coverage by default. A real agent is added only where delivery into the agent could fail.

## Command interface

The orchestration entry point is:

```console
cargo xtask agent-test [OPTIONS]
```

Initial options are:

```text
--list
--agent <agent>
--environment <host|container>
--scenario <name> ...
--symposium-bin <path>
--auth <api-key|local>
--max-agent-turns <count>
--max-input-tokens <count>
--max-output-tokens <count>
--max-tool-calls <count>
--keep-artifacts
```

`--scenario` is repeatable. No scenario means “print the execution plan,” not “start an agent.” A selection containing a real-agent journey requires an explicit agent. Missing runtime, credentials, or capability yields `Unavailable`; a requested container never silently falls back to the host.

The plan reports CLI-only and real-agent scenarios, scenario and operator token limits, maximum turns and tool calls, remaining daily and monthly capacity, environment, binary provenance, and pinned runtime before execution.

## Cost and runtime controls

Each real-agent scenario declares maximum cumulative input, cache-read, cache-write, and output tokens; provider requests; user turns; tool calls; real-time deadline; and usage class. Scheduled paid execution requires trustworthy provider accounting. Cached tokens remain visible and count toward token limits even when their billable price is lower.

Scenario-owned limits define the product contract. Exceeding one is `Failed`. Operator flags and run-wide limits are protective ceilings. If a lower `--max-agent-turns` or run-wide cap stops an otherwise valid scenario, the result is `InfrastructureError` owned by `runner.budget`, never a Symposium failure. The execution plan shows both limits and their effective minimum before paid work begins.

Before any provider request, the runner estimates harness-controlled prompt, fixture, skill, and tool context and rejects an oversized request as `InfrastructureError` owned by `runner.budget`. The adapter accounts for agent-owned context that can only be measured by the provider. A changed agent or model pin returns to manual calibration rather than inheriting the previous allowance.

The runner reserves the complete scenario ceiling from daily and monthly token ledgers before starting paid work. If either ledger lacks capacity, no provider request is made. Actual usage is charged after the run and unused capacity is released. A retry requires a second complete reservation; a billable first attempt is never retried when the remaining ledger cannot cover it.

CI uses a dedicated restricted provider key with a $5 monthly provider-side spending limit as the final backstop. Real-agent concurrency begins at one. Any follow-up latest-agent canary receives a smaller budget than pinned conformance. Credentials alone never enable paid tests; ordinary `cargo test` retains its explicit agent-testing gate.

Runtime reporting separates checkout build or image preparation, warm environment startup, agent execution, and assertion/evidence processing. This makes container overhead distinguishable from provider latency.

The tracer begins with five manually triggered calibration runs. Its provisional guard permits one user turn, at most four provider requests, three tool calls, 25,000 total input-side tokens across base input, cache reads, and cache writes, and 1,000 cumulative output tokens. It reserves 30,000 total tokens per day and 750,000 per month, allowing at most one paid run per day and 25 per month. If the pinned runtime cannot produce the nonce witness within that guard, the prompt, tools, fixture, and context are reduced before any limit is raised.

The initial scheduled limit is the maximum observed calibration usage plus 20 percent, never more than the provisional guard. After 20 eligible runs, it may be reviewed against P95 usage plus 20 percent. Limits never rise automatically after an agent upgrade or unusual run.

At the standard Sonnet base price of $3 per million input tokens and $15 per million output tokens, the provisional base-token estimate is approximately $0.09 per run. The runner allows up to $0.20 per run for cache-price differences while the provider key caps the month at $5. Actual cost should be lower after calibration. The estimate is recalculated when the model pin or [provider pricing](https://www.anthropic.com/news/claude-sonnet-5) changes; tokens remain the primary limit and dollars are derived reporting.

## CI lanes in this RFD

- Fast deterministic tests block every pull request.
- Stable native agent-free process and PTY scenarios may become PR-blocking.
- A small agent-free Linux-container suite may graduate if its measured runtime is acceptable.
- Real-agent tracer journeys run only in trusted scheduled or manual jobs during this RFD.

The nightly cadence produces the first 20 eligible samples in about 20 days when infrastructure is available.

## Post-tracer release policy

Assertion and Symposium failures are never retried to improve a product result. Each scheduled execution is an independent sample from fresh state. A real-agent journey becomes release-gating only after at least 19 of its latest 20 eligible scheduled executions pass for the same scenario contract and pinned adapter/runtime manifest. At least one eligible execution in that window must exercise the release-candidate revision. `InfrastructureError`, `Unavailable`, and `non-authoritative` runs do not enter the product pass-rate denominator; their rates are reported separately and they cannot satisfy the release-candidate requirement.

Falling below the target triggers a quarantine decision. Until reviewed, the release gate remains blocked. An approved quarantine requires a linked issue, continues running and reporting the journey, and excludes it from the gate. Adding or extending a quarantine requires review. A release gate evaluates the rolling target, not whether someone manually reran the latest failure.

Agent-free tests graduate after an observation period with no unexplained flakes, acceptable runtime, actionable failure artifacts, reliable cleanup, and consistently successful secret-canary validation. Provider credentials are never exposed to fork pull requests.

## Milestones and follow-on direction

### Tracer proven

The dependency-consent journey reuses the current fixture infrastructure and runs real `init` and `sync` processes. Accept and decline work through a parsed PTY, structured events agree with final state, the scenario runs in a fresh Linux container, and the accepted branch produces a persistent-Claude capability witness. Failure artifacts, cleanup, cost, and phase timing are demonstrated.

This validates the architecture and completes this RFD.

### Post-tracer direction

After the tracer, tracked follow-ups can expand the contract table across registry, discovery, predicate, cache, and delivery behavior. They can add every consent branch, representative Linux-container and native Windows/macOS lanes, fake and fixture-ACP adapter contracts, and selected hook and MCP witnesses.

Catalog automation, full release reporting, a latest-agent canary, and broader CI graduation are separate commitments informed by tracer evidence. They are not acceptance criteria for this RFD.

## Implementation plan

### Step 1: Run the first black-box host journey

From an empty user configuration, run compiled `cargo agents init --add-agent <agent>` and `cargo agents sync` processes under a PTY for one dependency candidate. Add only the scenario steps, side-channel events, terminal anchors, assertions, and artifacts required for initialization plus accept and decline.

Verify both branches against terminal output, structured events, exit status, configuration, filesystem state, the host-state canary, and the exact fixture-controlled custom-skill inventory. Add the deterministic assertion that invoking a hook through pipes writes only valid protocol output to stdout.

### Step 2: Record the tracer contracts

Write the initial Markdown contract table from the behavior exercised by step 1. A discrepancy becomes `Gap(issue)` only when it has a product issue and executable reproducer; otherwise it remains follow-on direction. Do not add catalog code generation or validation.

Verify manually that every `Covered` row names an executable scenario and every `Gap(issue)` row names both an issue and a reproducer.

### Step 3: Isolate the journey in Linux

Add Docker execution, the content-addressed Symposium binary, fixture services, least-privilege rules, and infrastructure diagnostics. Run the existing consent scenario unchanged.

Verify cold preparation, warm startup, the same host-state canary and custom-skill inventory assertions used by the host backend, and parity with the remaining host assertions.

### Step 4: Add the first real-agent witness

Add the persistent Claude adapter and extend the accepted branch with a fixture-skill witness. Pin its runtime and retain one interactive Claude smoke test.

Verify the capability nonce, persistent session, installation and hook-registration evidence, redaction, and error classification.

Record scheduled outcomes without automatic assertion retries. The tracer remains non-gating while it accumulates reliability evidence.

Before closing the RFD, correct `md/design/running-tests.md` so it documents the `SYMPOSIUM_ENABLE_AGENT_TESTING` gate. Keep `TestMode::AgentOnly`, `test-agents.toml`, and `tests/agent_harness/run_scenario.py` temporarily for existing Claude and ACP coverage, but mark that path as superseded and add no new scenarios to it. File its removal with the ACP follow-up, after remaining scenarios migrate.

Also file follow-up issues or RFDs for catalog automation, fake and ACP conformance, remaining scenario families, native operating-system expansion, and release CI graduation.
