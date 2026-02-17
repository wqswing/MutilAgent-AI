# LLM Intent Router Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace keyword-hardcoded intent routing with an LLM capability-driven classifier, while keeping deterministic fallback for resilience.

**Architecture:** Extend `DefaultRouter` to support an optional LLM classifier backed by runtime tool capabilities from `ToolRegistry::list()`. LLM classification is attempted first and must pass confidence + tool-existence checks. If LLM is unavailable or invalid, fallback to the current rule router to preserve uptime.

**Tech Stack:** Rust, async-trait, serde/serde_json, existing `LlmClient` + `ToolRegistry` traits.

---

### Task 1: Add failing tests for LLM-first routing behavior

**Files:**
- Modify: `crates/gateway/src/router.rs`

**Step 1: Write failing tests**

Add tests for:
- LLM returns valid `fast_action` with known tool -> router returns `UserIntent::FastAction`.
- LLM returns unknown tool -> router falls back to legacy rule routing.
- LLM returns low confidence -> router falls back to legacy routing.

**Step 2: Run test to verify it fails**

Run: `cargo test -p multi_agent_gateway router::tests::test_llm` 
Expected: FAIL because LLM path/config does not exist yet.

### Task 2: Implement LLM capability-driven router path

**Files:**
- Modify: `crates/gateway/src/router.rs`

**Step 1: Add LLM classifier fields and builder**
- Add optional fields to `DefaultRouter`:
- `llm_client: Option<Arc<dyn LlmClient>>`
- `tool_registry: Option<Arc<dyn ToolRegistry>>`
- `llm_min_confidence: f32`
- Add builder method: `with_llm_classifier(...)`.

**Step 2: Implement structured LLM decision parsing**
- Introduce local response struct for JSON decode.
- Add robust JSON extraction (raw JSON and fenced/extra-text responses).

**Step 3: Implement LLM-first classify flow**
- Build capability context from `ToolRegistry::list()`.
- Call `LlmClient::chat()` with strict JSON instruction.
- Accept only when:
- `confidence >= llm_min_confidence`
- Tool exists for `fast_action`.
- Else fallback to legacy keyword route.

**Step 4: Run targeted tests**

Run: `cargo test -p multi_agent_gateway router::tests`
Expected: PASS.

### Task 3: Wire runtime to LLM-based classifier

**Files:**
- Modify: `src/main.rs`

**Step 1: Construct router with LLM + tool registry**
- Create router after `llm_client` initialization.
- Pass `llm_client.clone()` and `tools.clone()` to `DefaultRouter::with_llm_classifier(...)`.

**Step 2: Verify compile**

Run: `cargo check --workspace --exclude cratesapp`
Expected: PASS.

### Task 4: Verification sweep

**Files:**
- No source changes required unless fixes needed.

**Step 1: Run component tests**

Run: `cargo test -p multi_agent_gateway router::tests`
Expected: PASS.

**Step 2: Run workspace compile gate**

Run: `cargo check --workspace --exclude cratesapp`
Expected: PASS.
