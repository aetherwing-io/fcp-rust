# fcp-rust

## Project Overview
MCP server that lets LLMs query and refactor Rust codebases through intent-level operation strings.
Rust implementation using rust-analyzer as the LSP backend (Tier 2 — native library via LSP).

## Architecture
4-layer architecture:
1. **MCP Server (Intent Layer)** — `src/mcp/server.rs` — Registers 4 MCP tools via rmcp, dispatches to domain
2. **Domain (Verbs)** — `src/domain/` — Query handlers, mutation handlers, verb registration
3. **Resolver (Symbol Resolution)** — `src/resolver/` — Multi-tier symbol resolution with selectors
4. **LSP Client** — `src/lsp/` — JSON-RPC communication with rust-analyzer subprocess
5. **FCP Core** — `src/fcpcore/` — Tokenizer, parsed-op, verb registry, event log, session, formatter

## Key Directories
- `src/mcp/` — MCP server setup, tool handlers
- `src/domain/` — Domain: model, verbs, query, mutation, format
- `src/resolver/` — Symbol resolution: selectors, index, pipeline, fuzzy
- `src/lsp/` — LSP client: transport, types, workspace edits, lifecycle
- `src/fcpcore/` — Shared FCP framework (Rust port of fcp-core)

## Commands
- `cargo test` — Run all tests
- `cargo build` — Build debug binary
- `cargo build --release` — Build release binary
- `cargo clippy -- -D warnings` — Lint check
- `make test` / `make build` / `make release` — via Makefile

## Conventions
- Rust 2021 edition, standard library style
- `rmcp` for MCP protocol (stdio transport)
- `rust-analyzer` spawned as subprocess for LSP
- Tests colocated with source (`#[cfg(test)]` modules)
- Integration tests in `tests/` (some `#[ignore]` requiring rust-analyzer)
