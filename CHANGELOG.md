# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [v1.0.4] - 2026-02-17

### 🚀 Major Features
- **Nexus Premium UI**: Complete dashboard overhaul with glassmorphism, neon accents, and improved layout.
- **Enhanced Governance**:
    - **Approval Timeline**: Visual tracking of human-in-the-loop decisions.
    - **Risk Scoring**: Real-time risk level indicators for sensitive agent actions.
- **Production Hardening**:
    - **Nonce-based Approval**: End-to-end replay protection for binary decisions.
    - **Encrypted Secrets Migration**: Automated migration and AES-256 encryption for provider keys.
    - **Egress Monitoring**: Real-time HTTP audit logging in the Research Agent.

## [v0.8.0] - 2026-01-18

### 🚀 Major Features
- **Enterprise Governance Layer**: 
    - Introduced a new `governance` crate managing Security, Audit, and Quotas.
    - Implemented `RbacConnector` trait with `OidcRbacConnector` (Keycloak/Auth0 RS256) and `NoOp` implementations.
    - Implemented `SecretsManager` trait with `AesGcmSecretsManager` (AES-256-GCM encryption).
    - Implemented `AuditStore` trait with `FileAuditStore` (JSON Lines persistence).

- **Admin Management Dashboard**:
    - Web-based UI at `/` serving static assets via `rust-embed`.
    - Features: Configuration inspector, Real-time Metrics view, and Audit Log explorer.
    - Protected by Bearer Token Authentication (verified against RBAC).

- **Observability**:
    - Integrated `metrics` and `metrics-exporter-prometheus` for real-time telemetry.
    - Admin API `/admin/metrics` endpoint exposes global system metrics.

### 🛡️ Security Hardening
- **Encryption at Rest**: All sensitive configuration secrets are now encrypted in memory and transit.
- **Audit Trails**: Critical system actions (config changes, access) are persistently logged.
- **Identity Integration**: Added support for external OIDC Identity Providers.

### 🔧 Improvements
- **Performance**: Upgraded `ArtifactStore` and `SessionStore` to use `DashMap` for high-concurrency read/write operations.
- **Testing**: Added comprehensive integration test suite (`tests/integration_v0_8.rs`) verifying the entire security and management pipeline.

## [v0.7.0] - 2024-12-XX

### 🚀 Major Features
- **Vector Database Integration**:
    - Added `qdrant-client` support for production-grade RAG workloads.
    - Implemented `QdrantMemoryStore` for persistent vector embeddings.

- **Advanced LLM Capabilities**:
    - Native support for **OpenAI Function Calling**, enabling structured and reliable tool execution.
    - Refactored `Controller` to use a unified `ActionParser` supporting both text-based (ReAct) and structured (JSON) outputs.

- **Architectural Refactoring**:
    - **Modular Traits**: Split monolithic core traits into domain-specific modules (`gateway`, `controller`, `skills`, `store`, `governance`, `llm`) to strictly enforce the 6-layer architecture.
    - **Mock Infrastructure**: Introduced `crates/core/src/mocks.rs` providing reusable mocks (`MockLlm`, `MockToolRegistry`, `RecordTool`) for all layers.

### 📦 Dependencies
- Upgraded `redis` crate to v0.27.
- Migrated to latest `aws-config` and `aws-sdk-s3`.
- Removed deprecated async connection methods.

## [v0.6.0] - 2024-11-XX
- Initial implementation of the 6-Layer Architecture.
- Basic ReAct Agent implementation.
- In-memory storage and naive semantic cache.
