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
