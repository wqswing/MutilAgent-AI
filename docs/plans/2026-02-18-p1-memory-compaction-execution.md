# P1 Memory Writeback + Compaction Execution Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement P1 memory writeback loop and compaction pre-flush so agent sessions persist useful memory artifacts and preserve critical state before context compaction.

**Architecture:** Add a dedicated filesystem writeback capability that writes session outcomes to daily logs and merges a canonical MEMORY.md. Upgrade compression capability from no-op to real session compaction, with a pre-compaction flush hook writing checkpoint records.

**Tech Stack:** Rust workspace, controller capabilities, tokio async tests, std filesystem.

---

### Task 1: Add Memory Writeback Capability (TDD)

**Files:**
- Create: `crates/controller/src/memory_writeback.rs`
- Modify: `crates/controller/src/lib.rs`
- Test: `crates/controller/tests/memory_writeback_test.rs`

**Step 1: Write failing integration tests**
- Verify on task finish it creates:
- `<memory_dir>/<YYYY-MM-DD>.md`
- `<memory_dir>/MEMORY.md`
- Verify MEMORY.md merge policy de-duplicates by session/goal key.

**Step 2: Run tests to verify failure**
- `cargo test -p multi_agent_controller --test memory_writeback_test -- --nocapture`

**Step 3: Implement capability**
- Add `MemoryWritebackCapability` implementing `AgentCapability::on_finish`.
- Append entries into daily file and merge into MEMORY.md.

**Step 4: Re-run tests**
- `cargo test -p multi_agent_controller --test memory_writeback_test -- --nocapture`

**Step 5: Commit**
- `git add crates/controller/src/memory_writeback.rs crates/controller/src/lib.rs crates/controller/tests/memory_writeback_test.rs`
- `git commit -m "feat(controller): add filesystem memory writeback capability"`

### Task 2: Pre-Compaction Flush + Effective Compaction (TDD)

**Files:**
- Modify: `crates/controller/src/capability.rs`
- Test: `crates/controller/tests/compaction_flush_test.rs`

**Step 1: Write failing integration tests**
- Verify when compression threshold is exceeded:
- session history is actually compacted.
- pre-compaction checkpoint is written to daily memory file.

**Step 2: Run tests to verify failure**
- `cargo test -p multi_agent_controller --test compaction_flush_test -- --nocapture`

**Step 3: Implement behavior**
- In `CompressionCapability.on_pre_reasoning`, call pre-compaction flush helper.
- Apply compressor output back into `session.history`.

**Step 4: Re-run tests**
- `cargo test -p multi_agent_controller --test compaction_flush_test -- --nocapture`

**Step 5: Commit**
- `git add crates/controller/src/capability.rs crates/controller/tests/compaction_flush_test.rs`
- `git commit -m "feat(controller): add pre-compaction flush and effective history compaction"`

### Task 3: Runtime Wiring + Verification

**Files:**
- Modify: `src/main.rs`
- Modify: `docs/plans/2026-02-18-p1-memory-compaction-execution.md`

**Step 1: Wire capability into runtime**
- Register `MemoryWritebackCapability` in controller builder.
- Ensure memory directory defaults to `.memory` (overridable by env).

**Step 2: Run verification**
- `cargo test -p multi_agent_controller -- --nocapture`
- `cargo check --workspace --exclude cratesapp`

**Step 3: Record outcomes and commit**
- Append pass/fail evidence and residual backlog into this plan doc.
- `git add src/main.rs docs/plans/2026-02-18-p1-memory-compaction-execution.md`
- `git commit -m "feat(runtime): wire memory writeback and record p1 verification"`

## Execution Result (2026-02-18)

### Delivered
- Added `MemoryWritebackCapability` to persist session outcomes into:
- `.memory/YYYY-MM-DD.md`
- `.memory/MEMORY.md`
- Implemented merge policy for `MEMORY.md` with deduplication on identical normalized entries.
- Added pre-compaction flush hook that writes `PRE-COMPACTION` checkpoints before history compression.
- Upgraded `CompressionCapability` to apply compressed messages back into `session.history` (effective compaction, no longer no-op).
- Wired runtime defaults in `src/main.rs`:
- `MemoryWritebackCapability::from_env()`
- `TruncationCompressor`
- Fixed legacy HITL test fixture to include `nonce` and `expires_at` in `ApprovalRequest`.

### Verification
- `cargo test -p multi_agent_controller -- --nocapture` -> PASS
- `cargo check --workspace --exclude cratesapp` -> PASS

### Residual for P2
- Explicit multi-agent routing policy model (`channel/account/peer`) and deterministic simulation tests.
- Skill ecosystem version/distribution governance (signature + compatibility gate).
