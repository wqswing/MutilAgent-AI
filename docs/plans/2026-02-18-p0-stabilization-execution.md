# P0 Stabilization Execution Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Remove current workspace test blockers and establish a hard CI-quality gate for P0.

**Architecture:** Apply two isolated fixes in parallel tracks: (1) complete missing test dependency wiring in `store`, (2) provide safe default config for `SafetyConfig` used by governed network tools. Then verify with package-level and workspace-level test gates.

**Tech Stack:** Rust workspace, Cargo, Axum ecosystem, tokio tests.

---

### Task 1: Fix `store` test dependency blocker

**Files:**
- Modify: `crates/store/Cargo.toml`

**Step 1: Reproduce failing test**
- Run: `cargo test -p multi_agent_store -- --nocapture`
- Expected: fail with unresolved import `tempfile`.

**Step 2: Add missing dev dependency**
- Add `tempfile = "3"` under `[dev-dependencies]`.

**Step 3: Verify package tests**
- Run: `cargo test -p multi_agent_store -- --nocapture`
- Expected: pass.

### Task 2: Fix `skills` config default blocker

**Files:**
- Modify: `crates/core/src/config.rs`

**Step 1: Reproduce failing package test**
- Run: `cargo test -p multi_agent_skills -- --nocapture`
- Expected: fail with `SafetyConfig::default` not found.

**Step 2: Add `Default` implementation**
- Implement `Default for SafetyConfig` with safe limits/content-types aligned to current app defaults.

**Step 3: Verify package tests**
- Run: `cargo test -p multi_agent_skills -- --nocapture`
- Expected: pass.

### Task 3: P0 verification gate

**Files:**
- No source changes required unless failures found.

**Step 1: Run workspace gate**
- Run: `cargo test --workspace`
- Expected: pass (or explicit remaining failures documented with root cause).

**Step 2: Record outcome**
- Include exact command results and residual risks in handoff summary.
