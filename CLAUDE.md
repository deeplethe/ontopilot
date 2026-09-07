# Utopia

Knowledge graph platform: document ingestion, entity/fact extraction, hybrid search, ontology management, and inference.

## Stack

- **Language**: Rust (workspace with 8 crates) + Next.js-style Vite/React frontend
- **Framework**: Axum 0.8 (HTTP server)
- **Database**: PostgreSQL 16 with pgvector (via sqlx 0.8)
- **Search**: Tantivy BM25 + pgvector cosine similarity, RRF fusion
- **Frontend**: React 18 + Vite + pnpm + TypeScript
- **Deploy**: kubernetes (`deploy: kubernetes`)

## Workspace Crates

| Crate | Purpose |
|-------|---------|
| `utopia-core` | Domain models, error types, config, secrets (AES-256-GCM) |
| `utopia-store` | sqlx repositories, migrations, job queue, test_db helpers |
| `utopia-server` | Axum HTTP server, API routes, auth (JWT + Argon2), pipeline |
| `utopia-ingest` | Document parsing (PDF, CSV, Excel, HTML, Markdown) and chunking |
| `utopia-extract` | LLM-driven entity/fact extraction from chunks |
| `utopia-reason` | Inference engine (rule-based reasoning over knowledge graph) |
| `utopia-search` | Tantivy full-text index, hybrid search with RRF fusion |
| `utopia-llm` | LLM client abstraction (chat, embed, extract) |

## Frontend

Located in `web/`. Vite + React + TypeScript + pnpm.

```bash
cd web && pnpm install && pnpm dev   # dev server on :5173
cd web && pnpm build                  # production build
```

## Commands

```bash
# Run all tests (needs UTOPIA_TEST_DB_URL for integration tests)
cargo test --workspace

# Format check
cargo fmt --all --check

# Lint
cargo clippy --workspace --all-targets -- -D warnings

# Build release
cargo build --workspace --release

# Docker (from repo root)
docker build -f docker/Dockerfile -t utopia .

# Frontend
cd web && pnpm build
```

## Environment Variables (prefix: `UTOPIA_`)

| Variable | Default | Description |
|----------|---------|-------------|
| `UTOPIA_DATABASE_URL` | — | Postgres connection (needs pgvector) |
| `UTOPIA_BIND_ADDR` | `0.0.0.0:1516` | Server bind address |
| `UTOPIA_JWT_SECRET` | auto-generated | JWT signing secret (HS256) |
| `UTOPIA_SECRET_KEY` | auto-generated | AES-256-GCM key for credential encryption |
| `UTOPIA_WEB_DIST` | — | Path to frontend build output |
| `UTOPIA_DATA_DIR` | — | Data directory (files + Tantivy index) |
| `UTOPIA_DB_MAX_CONNECTIONS` | `32` | Connection pool size |

## Testing

- Unit tests run without a database
- Integration tests need `UTOPIA_TEST_DB_URL=postgres://utopia:utopia@localhost:1517/utopia`
- Store integration tests in `crates/utopia-store/tests/` (50+ test files)
- Server tests in `crates/utopia-server/src/` (RSS contract tests, job route tests)
- Tests skip gracefully when DB URL is absent

## Auth System

- Passwords: Argon2 (default params)
- JWT: HS256, 7-day TTL (`TOKEN_TTL_DAYS = 7`)
- Session: `utopia_token` cookie (HttpOnly, SameSite=Lax)
- Personal access tokens: `utp_` prefix, scoped to read/write + optional KB restriction
- Roles: Viewer < Editor < Admin < Owner (per-KB membership)

## Downstream: Aletheia

Aletheia is a downstream Bayesian belief engine that reads from Utopia's API:
- `POST /api/v1/auth/login` — service account authentication
- `GET /api/v1/kbs/{kb}/entities/{entity}` — entity facts
- `GET /api/v1/kbs/{kb}/facts/{fact}/evidence` — passage evidence
- `POST /api/v1/kbs/{kb}/search` — hybrid search

Aletheia service account should be a non-admin user with Viewer role on the target KB.
