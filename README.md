# fcp-rust

MCP server for Rust codebases via rust-analyzer.

## What It Does

fcp-rust lets LLMs navigate, inspect, and refactor Rust code through intent-level commands. Instead of reading raw source files, the LLM sends operations like `find Config @kind:struct` or `rename old_name new_name @file:main.rs` and fcp-rust routes them through rust-analyzer's LSP for accurate, project-aware results. Built on the [FCP](https://github.com/os-tack/fcp) framework.

Written in Rust using [rmcp](https://github.com/anthropics/rmcp) for MCP transport and rust-analyzer as the language intelligence backend.

## Quick Example

```
rust_session('open /path/to/my-project')

rust_query('find Config @kind:struct')
rust_query('refs Config @file:main.rs')
rust_query('diagnose')

rust([
  'rename old_handler handle_request @file:routes.rs',
  'generate Display @struct:Config',
])

rust_query('map')
```

### Available MCP Tools

| Tool | Purpose |
|------|---------|
| `rust(ops)` | Batch mutations — rename, extract, inline, generate, import |
| `rust_query(q)` | Inspect the codebase — find, def, refs, symbols, diagnose, map |
| `rust_session(action)` | Lifecycle — open, status, close |
| `rust_help()` | Full reference card |
| `enrich(path)` | Extract diagnostics and symbols from a single .rs file (no session required) |

### Verb Reference — Navigation

| Verb | Syntax |
|------|--------|
| `find` | `find QUERY [kind:KIND] [@selectors...]` |
| `def` | `def SYMBOL [@selectors...]` |
| `refs` | `refs SYMBOL [@selectors...]` |
| `symbols` | `symbols PATH [kind:KIND]` |
| `impl` | `impl SYMBOL [@selectors...]` |

### Verb Reference — Inspection

| Verb | Syntax |
|------|--------|
| `diagnose` | `diagnose [PATH] [@all]` |
| `inspect` | `inspect SYMBOL [@selectors...]` |
| `callers` | `callers SYMBOL [@selectors...]` |
| `callees` | `callees SYMBOL [@selectors...]` |
| `map` | `map` |
| `unused` | `unused [@file:PATH]` |

### Verb Reference — Mutation

| Verb | Syntax |
|------|--------|
| `rename` | `rename SYMBOL NEW_NAME [@selectors...]` |
| `extract` | `extract FUNC_NAME @file:PATH @lines:N-M` |
| `inline` | `inline SYMBOL [@selectors...]` |
| `generate` | `generate TRAIT @struct:NAME` |
| `import` | `import SYMBOL @file:PATH @line:N` |

### Verb Reference — Session

| Verb | Syntax |
|------|--------|
| `open` | `open PATH` |
| `status` | `status` |
| `close` | `close` |

### Selectors

```
@file:PATH          Filter by file path substring
@struct:NAME        Filter by containing struct
@trait:NAME         Filter by containing trait
@kind:KIND          Filter by symbol kind (function, struct, enum, etc.)
@module:NAME        Filter by module name
@line:N             Filter by exact line number
@lines:N-M          Line range (for extract)
```

## Installation

### Build from source

```bash
cargo install --git https://github.com/os-tack/fcp-rust
```

### MCP Client Configuration

```json
{
  "mcpServers": {
    "fcp-rust": {
      "command": "fcp-rust"
    }
  }
}
```

## Architecture

```
src/main.rs                    MCP server — 4 tools, stdio transport
    │
src/mcp/                       MCP integration
  ├── server.rs                RustServer (rmcp tool handlers)
  └── params.rs                Request/response types
    │
src/domain/                    Domain layer
  ├── verbs.rs                 Verb registration (query + mutation + session)
  ├── model.rs                 RustModel (workspace state, LSP client, symbol index)
  ├── query.rs                 Query handlers (find, def, refs, diagnose, map, etc.)
  ├── mutation.rs              Mutation handlers (rename, extract, inline, generate)
  └── format.rs                Output formatting
    │
src/resolver/                  Symbol resolution
  ├── selectors.rs             @file, @struct, @trait, @kind, @module, @line, @lines
  ├── index.rs                 In-memory SymbolIndex
  ├── pipeline.rs              Multi-tier resolution (index → LSP → tree-walk)
  └── fuzzy.rs                 Fuzzy matching
    │
src/lsp/                       rust-analyzer integration
  ├── client.rs                JSON-RPC client
  ├── transport.rs             stdio transport
  ├── types.rs                 LSP type definitions
  ├── workspace_edit.rs        Apply edits to disk
  └── lifecycle.rs             Server lifecycle management
    │
src/fcpcore/                   FCP framework (Rust port)
  ├── tokenizer.rs             DSL tokenizer
  ├── parsed_op.rs             Operation parser
  ├── verb_registry.rs         Verb spec registry + reference card
  ├── event_log.rs             Event sourcing (undo/redo)
  ├── session.rs               Session lifecycle
  └── formatter.rs             Response formatter
```

## Development

```bash
cargo test          # Run all tests
cargo build         # Build binary
cargo clippy        # Run lints
```

## License

MIT
