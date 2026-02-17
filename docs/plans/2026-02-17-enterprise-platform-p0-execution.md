# Enterprise Platform P0 Execution Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Deliver enterprise-grade P0 foundations: typed gateway contract, idempotent side-effect APIs, and deterministic controller scheduling.

**Architecture:** Add a versioned typed contract in Core and use it in Gateway responses. Add lightweight idempotency middleware/service for side-effect endpoints. Add a dual-lane scheduler in Gateway that enforces per-session serialization with bounded global parallelism before invoking Controller.

**Tech Stack:** Rust workspace, Axum, Tokio, Serde/JSON Schema, async tests.

---

### Task 1: Typed Gateway Contract (Core)

**Files:**
- Create: `crates/core/src/types/gateway_contract.rs`
- Modify: `crates/core/src/types/mod.rs`

**Step 1: Write failing tests for contract serialization and error code stability**
- Add unit tests in `gateway_contract.rs` for:
- envelope serialization includes `version`, `trace_id`, `data`.
- error response serialization includes stable `code`, `message`, `retryable`.

**Step 2: Run tests to verify failure**
- Run: `cargo test -p multi_agent_core gateway_contract -- --nocapture`
- Expected: tests fail before implementation.

**Step 3: Implement contract types**
- Add:
- `ApiVersion`, `ApiEnvelope<T>`, `ApiErrorBody`, `ApiErrorCode`.
- helper constructors for success/error.

**Step 4: Run tests to verify pass**
- Run: `cargo test -p multi_agent_core gateway_contract -- --nocapture`

**Step 5: Commit**
- `git add crates/core/src/types/gateway_contract.rs crates/core/src/types/mod.rs`
- `git commit -m "feat(core): add versioned gateway typed contract"`

### Task 2: Gateway Contract Adoption + Schema Endpoint

**Files:**
- Modify: `crates/gateway/src/server.rs`
- Modify: `crates/gateway/src/lib.rs`

**Step 1: Write failing tests for response envelope and schema endpoint**
- Add/extend tests in `crates/gateway/tests/gateway_test.rs`:
- `/v1/intent` returns envelope with `version` and `data.intent`.
- `/v1/system/schema/gateway` returns schema metadata.

**Step 2: Run tests to verify failure**
- Run: `cargo test -p multi_agent_gateway gateway_test -- --nocapture`

**Step 3: Implement adoption**
- Add route `/v1/system/schema/gateway`.
- Change `chat`, `intent`, `webhook` handlers to return `ApiEnvelope`.
- Map internal errors to typed `ApiErrorBody` with stable error codes.

**Step 4: Run tests to verify pass**
- Run: `cargo test -p multi_agent_gateway gateway_test -- --nocapture`

**Step 5: Commit**
- `git add crates/gateway/src/server.rs crates/gateway/src/lib.rs crates/gateway/tests/gateway_test.rs`
- `git commit -m "feat(gateway): adopt typed envelopes and publish schema endpoint"`

### Task 3: Idempotency for Side-Effect Endpoints

**Files:**
- Create: `crates/gateway/src/idempotency.rs`
- Modify: `crates/gateway/src/server.rs`
- Modify: `crates/gateway/src/lib.rs`
- Test: `crates/gateway/tests/gateway_test.rs`

**Step 1: Write failing tests for webhook/approve idempotency**
- Add tests with `Idempotency-Key` header:
- same key + same payload returns same stored response.
- same key + different payload returns conflict.

**Step 2: Run tests to verify failure**
- Run: `cargo test -p multi_agent_gateway idempotency -- --nocapture`

**Step 3: Implement minimal idempotency store**
- in-memory keyed by `(endpoint, key)` with payload hash + serialized response body + status.
- Apply to `/v1/webhook/{event_type}` and `/v1/approve/{request_id}`.

**Step 4: Run tests to verify pass**
- Run: `cargo test -p multi_agent_gateway idempotency -- --nocapture`

**Step 5: Commit**
- `git add crates/gateway/src/idempotency.rs crates/gateway/src/server.rs crates/gateway/src/lib.rs crates/gateway/tests/gateway_test.rs`
- `git commit -m "feat(gateway): add idempotency keys for side-effect endpoints"`

### Task 4: Session Lane + Global Lane Scheduler

**Files:**
- Create: `crates/gateway/src/scheduler.rs`
- Modify: `crates/gateway/src/server.rs`
- Modify: `crates/gateway/src/lib.rs`
- Test: `crates/gateway/tests/gateway_test.rs`

**Step 1: Write failing tests for serialized session execution**
- Add test using mock controller with counters:
- same session requests never overlap.
- different sessions can overlap up to global limit.

**Step 2: Run tests to verify failure**
- Run: `cargo test -p multi_agent_gateway scheduler -- --nocapture`

**Step 3: Implement scheduler**
- `global_semaphore` + per-session mutex map.
- integrate only around controller execution path in chat handler.

**Step 4: Run tests to verify pass**
- Run: `cargo test -p multi_agent_gateway scheduler -- --nocapture`

**Step 5: Commit**
- `git add crates/gateway/src/scheduler.rs crates/gateway/src/server.rs crates/gateway/src/lib.rs crates/gateway/tests/gateway_test.rs`
- `git commit -m "feat(gateway): add session lane and global lane controller scheduler"`

### Task 5: Verification and P1/P2 Backlog Handoff

**Files:**
- Modify: `docs/plans/2026-02-17-enterprise-platform-p0-execution.md`

**Step 1: Run verification suite**
- `cargo check --workspace --exclude cratesapp`
- `cargo test -p multi_agent_core`
- `cargo test -p multi_agent_gateway`

**Step 2: Record results and unresolved items**
- Append verification outcomes and residual P1/P2 backlog to plan doc.

**Step 3: Commit**
- `git add docs/plans/2026-02-17-enterprise-platform-p0-execution.md`
- `git commit -m "docs: record p0 verification and next-stage backlog"`
