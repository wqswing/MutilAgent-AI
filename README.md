# Multiagent: Advanced Multi-Agent AI System

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Language](https://img.shields.io/badge/rust-1.75%2B-blue.svg)](https://www.rust-lang.org)
[![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)](https://github.com/wqswing/Multiagent-AI/actions)
[![Docker](https://img.shields.io/badge/docker-ready-blue.svg)](Dockerfile)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](CONTRIBUTING.md)
[![Version](https://img.shields.io/badge/version-0.7.0-orange.svg)](https://github.com/wqswing/Multiagent-AI/releases/tag/v0.7)

Multiagent is a production-grade, layered AI agent framework built in Rust. It is designed for high-performance orchestration of LLM capabilities, supporting multi-modal inputs, autonomous reasoning (ReAct), complex workflow automation (DAG/SOP), and robust enterprise features like semantic caching, vector memory, and circuit breakers.

## ✨ What's New in v0.7

- **Qdrant Vector Database**: Production-ready vector store for RAG workloads
- **OpenAI Function Calling**: Native support for structured tool calls
- **Modular Traits**: Reorganized core traits by architectural layer
- **Mock Infrastructure**: Comprehensive testing mocks for all components
- **Dependency Updates**: Redis v0.27, latest AWS SDK

## 🚀 Key Capabilities

### 🧠 Intelligence Layer
- **Autonomous Agents**: Uses the ReAct (Reason+Act) pattern to solve complex, multi-step problems
- **Workflow Orchestration**: Supports parallel execution of tasks via DAGs and SOPs defined in YAML
- **Adaptive Model Selection**: Dynamically routes requests to the best LLM based on complexity and cost
- **Long-Term Memory**: RAG-enabled memory with Qdrant vector database integration

### ⚡ Performance & Scalability
- **Semantic Caching**: Vector-embedding based caching to serve repeated queries instantly
- **Tiered Storage**: Hybrid storage using In-Memory (fast), Redis (state), and S3 (artifacts)
- **Circuit Breaker**: Automatic failure detection and isolation for LLM providers

### 👁️ Multi-Modal Support
- **Vision**: Ingest and process images for visual reasoning
- **Audio**: Integrated Whisper support for speech-to-text transcription
- **MCP Host**: Full support for Model Context Protocol to connect external tools

## 🏗️ Architecture

Multiagent follows a strict 6-layer architecture for separation of concerns and scalability.

### Layer Architecture
```
┌─────────────────────────────────────────────────────────────┐
│                     User / Client                            │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│  L0: Gateway Layer                                           │
│  ┌─────────────┐ ┌─────────────┐ ┌────────────────┐         │
│  │ Axum Server │ │Intent Router│ │ Semantic Cache │         │
│  └─────────────┘ └─────────────┘ └────────────────┘         │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│  L1: Controller Layer                                        │
│  ┌─────────────┐ ┌─────────────┐ ┌────────────────┐         │
│  │ReAct Engine │ │  Capabilties│ │ Parser/Executor│         │
│  └─────────────┘ └─────────────┘ └────────────────┘         │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│  L2: Skills Layer                                            │
│  ┌─────────────┐ ┌─────────────┐ ┌────────────────┐         │
│  │Tool Registry│ │ MCP Adapter │ │ Builtin Tools  │         │
│  └─────────────┘ └─────────────┘ └────────────────┘         │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│  L3: Store Layer                                             │
│  ┌─────────────┐ ┌─────────────┐ ┌────────────────┐         │
│  │  In-Memory  │ │ Redis/S3    │ │ Qdrant Vector  │         │
│  └─────────────┘ └─────────────┘ └────────────────┘         │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│  L4: Governance Layer                                        │
│  ┌─────────────┐ ┌─────────────┐ ┌────────────────┐         │
│  │ Guardrails  │ │Token Budget │ │ Metrics/Trace  │         │
│  └─────────────┘ └─────────────┘ └────────────────┘         │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│  L-M: Model Gateway                                          │
│  ┌─────────────┐ ┌─────────────┐ ┌────────────────┐         │
│  │   Selector  │ │Circuit Break│ │ OpenAI/Claude  │         │
│  └─────────────┘ └─────────────┘ └────────────────┘         │
└─────────────────────────────────────────────────────────────┘
```

### Request Flow

```mermaid
sequenceDiagram
    participant User
    participant Gateway as L0: Gateway
    participant Cache as Semantic Cache
    participant Router as Intent Router
    participant Controller as L1: Controller
    participant LLM as L-M: Model Gateway
    participant Tools as L2: Skills
    participant Store as L3: Store

    User->>Gateway: POST /v1/chat
    Gateway->>Cache: Check cache
    
    alt Cache Hit
        Cache-->>Gateway: Cached response
        Gateway-->>User: Return cached
    else Cache Miss
        Gateway->>Router: Classify intent
        
        alt FastAction
            Router-->>Gateway: FastAction{tool, args}
            Gateway->>Tools: Execute tool directly
            Tools-->>Gateway: ToolOutput
        else ComplexMission
            Router-->>Gateway: ComplexMission{goal}
            Gateway->>Controller: Execute ReAct loop
            
            loop ReAct Cycle (max N iterations)
                Controller->>Store: Load memory/context
                Controller->>LLM: Generate thought
                LLM-->>Controller: THOUGHT + ACTION
                Controller->>Tools: Execute action
                Tools-->>Controller: Observation
                Controller->>Controller: Update state
            end
            
            Controller-->>Gateway: FINAL ANSWER
        end
        
        Gateway->>Cache: Store result
        Gateway-->>User: Return response
    end
```


## 📂 Project Structure

```
crates/
├── core/           # Shared traits & types
│   ├── traits/     # Modular trait definitions by layer
│   ├── types/      # AgentResult, Session, ToolOutput
│   └── mocks.rs    # Test mocks for all components
├── gateway/        # Axum server, Semantic Cache, Router
├── controller/     # ReAct loop, Parser, Executor
├── skills/         # Tool Registry, MCP Adapter
├── store/          # Redis, S3, Qdrant implementations
├── governance/     # Guardrails, Budget, Metrics
└── model_gateway/  # LLM Provider integration
```

## 🛠️ Getting Started

### Prerequisites
- **Rust**: 1.75+
- **Docker**: For dependencies (Redis, Qdrant, Jaeger)
- **API Keys**: OpenAI or Anthropic

### Environment Setup

```bash
# LLM Providers
export OPENAI_API_KEY=sk-...
export ANTHROPIC_API_KEY=sk-...

# Persistence (Optional)
export REDIS_URL=redis://localhost:6379
export QDRANT_URL=http://localhost:6334

# Observability
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
```

### Running Locally

```bash
# Start infrastructure
docker-compose up -d

# Run the agent
cargo run

# Run tests
cargo test --workspace
```

The server listens on `http://0.0.0.0:3000`.

## 📖 Usage Examples

### Chat (ReAct Agent)
```bash
curl -X POST http://localhost:3000/v1/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "Analyze this dataset and create a summary report."}'
```

### Fast Intent (Direct Tool)
```bash
curl -X POST http://localhost:3000/v1/intent \
  -H "Content-Type: application/json" \
  -d '{"message": "Calculate 123 * 456"}'
```

### Health & Metrics
```bash
curl http://localhost:3000/health
curl http://localhost:3000/metrics
```

## 🧪 Testing

Multiagent includes comprehensive testing infrastructure:

```rust
use multi_agent_core::mocks::{MockLlm, MockToolRegistry, MockMemoryStore};

// Create deterministic LLM for testing
let llm = MockLlm::new(vec![
    "THOUGHT: Analyzing...".to_string(),
    "FINAL ANSWER: Done".to_string(),
]);

// Create recording tools
let tool = RecordingTool::new("search", "Search the web", "Results...");
```

## 📄 License

MIT License - See [LICENSE](LICENSE) for details.

Copyright (c) 2024-2026 Multiagent Contributors
