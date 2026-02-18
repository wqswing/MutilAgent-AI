# P2 Routing Policy + Skill Governance Execution Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Deliver explicit `channel/account/peer` routing policy with deterministic simulation tests, and in parallel add minimal skill ecosystem governance for versioned distribution.

**Architecture:** Add a policy module in Gateway that resolves rule conflicts through explicit precedence and deterministic tie-breakers, then integrate it into DefaultRouter before LLM/fallback routing. In parallel, extend plugin manifest schema with distribution channel and runtime compatibility checks.

**Tech Stack:** Rust, Axum/Gateway router, serde/schemars, tokio tests, semver.

---

### Task 1: Explicit routing policy model + simulator (TDD)

**Files:**
- Create: `crates/gateway/src/routing_policy.rs`
- Modify: `crates/gateway/src/lib.rs`
- Test: `crates/gateway/tests/routing_policy_sim_test.rs`

**Steps:**
1. Write failing tests for precedence `channel > account > peer`, deterministic tie-break, and simulation output.
2. Run: `cargo test -p multi_agent_gateway --test routing_policy_sim_test -- --nocapture` (expect fail).
3. Implement policy structs + `simulate(...)` resolver.
4. Re-run test to pass.
5. Commit.

### Task 2: Integrate policy into DefaultRouter (TDD)

**Files:**
- Modify: `crates/gateway/src/router.rs`
- Test: `crates/gateway/src/router.rs` (unit tests)

**Steps:**
1. Add failing router tests: policy can force fast_action/tool and complex_mission, and diagnostics include policy scope/rule id.
2. Run targeted router tests (expect fail).
3. Implement policy-aware routing path before LLM/rule fallback.
4. Re-run targeted router tests to pass.
5. Commit.

### Task 3: Skill ecosystem governance baseline (parallel track, TDD)

**Files:**
- Modify: `crates/ecosystem/src/manifest.rs`
- Modify: `crates/ecosystem/Cargo.toml`
- Test: `crates/ecosystem/src/manifest.rs` (unit tests)

**Steps:**
1. Write failing tests for manifest validation:
- semver validity
- runtime compatibility
- allowed distribution channel
2. Run: `cargo test -p multi_agent_ecosystem manifest -- --nocapture` (expect fail).
3. Implement validation API and fields (`distribution_channel`, `min_runtime_version`, optional `signature`).
4. Re-run tests to pass.
5. Commit.

### Task 4: Verification + report

**Files:**
- Modify: `docs/plans/2026-02-18-p2-routing-governance-execution.md`

**Steps:**
1. Run:
- `cargo test -p multi_agent_gateway -- --nocapture`
- `cargo test -p multi_agent_ecosystem -- --nocapture`
- `cargo check --workspace --exclude cratesapp`
2. Record outcomes and residual P2 backlog.
3. Commit.

## Execution Result (2026-02-18)

### Delivered
- Added explicit routing policy model for `channel/account/peer` in Gateway.
- Added deterministic resolver with explicit precedence and tie-break:
- scope precedence: `channel > account > peer`
- then higher numeric `priority`
- then lexical `rule_id` for deterministic conflict resolution
- Added simulation API for policy scenarios.
- Integrated policy path into `DefaultRouter` before LLM/fallback routing.
- Added router diagnostics for policy path: `source=policy`, `scope`, `rule_id`.

### Parallel Delivered (Follow-on Task)
- Added plugin manifest governance baseline:
- semver validation for plugin version
- runtime compatibility check (`min_runtime_version`)
- distribution channel allowlist (`stable|canary`)
- optional signature prefix validation (`sha256:` or `ed25519:`)
- Enforced manifest validation during plugin install and plugin scan.

### Verification
- `cargo test -p multi_agent_gateway -- --nocapture` -> PASS
- `cargo test -p multi_agent_ecosystem -- --nocapture` -> PASS
- `cargo check --workspace --exclude cratesapp` -> PASS

### Remaining P2 Backlog
- Policy simulation endpoint for admin plane (`/v1/admin/routing/simulate`).
- Policy storage/versioning and staged rollout controls.
- Signature verification against trusted keyring (actual cryptographic verification, not prefix format gate).

## Extension Result (Admin Routing APIs)

### Delivered
- Added admin management API for routing policy lifecycle:
- `POST /v1/admin/routing/publish`
- `POST /v1/admin/routing/simulate`
- `GET /v1/admin/routing/policies`
- Added versioned routing policy store with release history and monotonic semver publish checks.
- Wired shared store between runtime router and admin API so published policy is live immediately.

### Verification
- `cargo test -p multi_agent_gateway -- --nocapture` -> PASS
- `cargo test -p multi_agent_ecosystem -- --nocapture` -> PASS
- `cargo check --workspace --exclude cratesapp` -> PASS
