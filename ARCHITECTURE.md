# Multiagent v0.7 Architecture Overview

## 1. System Overview

Multiagent follows a strict layered architecture (`L0` to `L4`) to separate concerns between connectivity, orchestration, execution, storage, and governance.

### High-Level Layers

| Layer | Name | Responsibility | Key Crates |
| :--- | :--- | :--- | :--- |
| **L0** | **Gateway** | Connectivity, Protocol Handling, Initial Routing | `multi_agent_gateway` |
| **L1** | **Controller** | Orchestration, Reasoning, Task Management | `multi_agent_controller` |
| **L2** | **Skills** | Tools, MCP Integration, Atomic Actions | `multi_agent_skills` |
| **L3** | **Store** | Persistence (Sessions, Artifacts, Knowledge) | `multi_agent_store` |
| **L4** | **Governance** | Observability, Security, Budgets | `multi_agent_governance` |
| **L-M** | **Model Gateway** | LLM Abstraction, Load Balancing | `multi_agent_model_gateway` |
| **Core** | **Core** | Shared Types, Traits, Error Handling | `multi_agent_core` |

---

## 2. Component Detail & Logical Associations

### L0: Gateway (`crates/gateway`)
- **Entry Point**: `GatewayServer` (Axum).
- **Responsibility**: Accepts HTTP/WS requests, checks `SemanticCache`, parses requests into `NormalizedRequest`.
- **Associations**:
  - Uses `IntentRouter` (Trait) to classify user intent.
  - Wraps `Controller` (Trait) to execute complex tasks.
  - **Decoupling**: Does not know about specific controller implementations (e.g., ReAct).

### L1: Controller (`crates/controller`)
- **Core Logic**: `ReActController` implements the ReAct loop.
- **Capabilities**: Modular system for extending agent behavior (`AgentCapability` trait).
  - `SecurityCapability`: Checks inputs/outputs.
  - `MemoryCapability`: RAG context injection.
  - `PlanningCapability`: Chain-of-thought planning.
- **Associations**:
  - Holds `Arc<dyn ToolRegistry>` to execute tools (L2).
  - Holds `Arc<dyn SessionStore>` to save state (L3).
  - Holds `Arc<dyn LlmClient>` to reason (L-M).

### L2: Skills (`crates/skills`)
- **Tooling**: `DefaultToolRegistry` (runtime) and `McpRegistry` (remote).
- **Unified Interface**: `CompositeToolRegistry` aggregates multiple registries, allowing transparent access to local and MCP tools.
- **Associations**:
  - Implements `Tool` and `ToolRegistry` traits from Core.
  - Used by Controller to perform actions.

### L3: Store (`crates/store`)
- **Persistence**:
  - `ArtifactStore`: S3/Memory for files.
  - `SessionStore`: Redis/Memory for agent state.
  - `MemoryStore` (New): Vector database for RAG.
- **Associations**:
  - Used by `src/main.rs` to configure the system.
  - Used by Controller capabilities (e.g., Memory uses `MemoryStore`).

### L-M: Model Gateway (`crates/model_gateway`)
- **LLM Access**: `RigLlmClient` wraps `rig-core`.
- **Associations**:
  - Provides `LlmClient` implementation used by Controller and Gateway (for embeddings).

---

## 3. Data Flow (Chat Configuration)

1. **Request**: User POSTs to `/v1/chat`.
2. **Gateway**:
   - Checks `SemanticCache`. If hit -> Return.
   - `IntentRouter` classifies `NormalizedRequest`.
   - if `FastAction` -> Gateway executes directly (if configured) or passes to Controller.
   - if `ComplexMission` -> Calls `Controller::execute`.
3. **Controller** (`ReActController`):
   - **OnStart**: Capabilities (Security, Memory) run hooks.
   - **Reasoning**: Calls `LlmClient` with history + system prompt.
   - **Action**: Parses `ACTION: tool`.
   - **Execution**: Calls `ToolRegistry::execute`.
   - **Observation**: Tool output added to history.
   - **Loop**: Repeats until `FINAL ANSWER`.
4. **Response**: Result returned to Gateway, cached, and sent to user.

---

## 4. Current State (v0.7 Initialization)

- **Optimized**: `HistoryEntry` now uses `Arc<String>` for performance.
- **Unified**: Local and MCP tools share a single registry interface.
- **Modular**: Capabilities allow plugging in new features (e.g., specialized planning) without touching the core loop.

## 5. Directory Structure
```
Multiagent/
├── crates/
│   ├── core/         # Shared traits/types
│   ├── controller/   # ReAct loop, Capabilities
│   ├── gateway/      # API Server, Router
│   ├── skills/       # Tools, MCP
│   ├── store/        # Redis, S3, Vector
│   ├── governance/   # Metrics, Security
│   └── model_gateway/# LLM Clients
├── src/
│   └── main.rs       # Wiring & Entry Point
└── tests/            # Integration Tests
```
