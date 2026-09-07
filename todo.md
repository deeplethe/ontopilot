# Utopia Todo

## Production Readiness (workspace deployment)

- [ ] Sync `dev` branch with `origin/main` — resolve any divergence, ensure workspace checkout is on the latest stable branch.
- [x] Add `CLAUDE.md` with galaxy-specific agent instructions: Rust workspace + Next.js frontend, `cargo test --workspace`, `pnpm build`, env vars (`UTOPIA_*`), pgvector dependency, deploy target kubernetes, Aletheia downstream relationship.
- [x] Dockerfile: added non-root `USER utopia` directive with `groupadd`/`useradd` and `chown` on `/app`.

## Aletheia Integration — API Contract Verification (must work perfectly)

### Auth Contract

- [x] Verify `POST /api/v1/auth/login` returns `{ "user": ..., "token": string }`. Aletheia's `LoginResponse { token }` correctly picks up `token` via serde (ignores extra fields). Contract test on both sides.
- [x] Verify JWT expiry is 7 days: both sides hardcode `TOKEN_TTL_DAYS = 7`. Confirmed in `auth.rs:21`.
- [x] Verify expired JWT returns HTTP 401: `AuthUser` extractor returns `AppError::Unauthorized` → 401. Confirmed in `auth.rs:132`.

### Entity Detail Contract

- [x] Verify `GET /api/v1/kbs/{kb}/entities/{entity}` response includes all fields Aletheia deserializes. `GraphNode` serializes as `name` (not `canonical_name`); Aletheia handles via `#[serde(alias = "name")]`. Contract tests on both sides verify all 11 required fields.
- [x] Write contract tests: serialization tests in `utopia-core/src/models.rs` verify all required fields and types.
- [x] Ensure `confidence` is always returned as a float: `EntityFact.confidence` is `f32`, serializes as JSON number. Contract test asserts `is_f64()`.

### Fact Evidence Contract

- [x] Verify `GET /api/v1/kbs/{kb}/facts/{fact}/evidence` returns envelope `{ "evidence": [...] }`. Confirmed in `graph_routes.rs:442`. Contract tests on both sides.
- [x] Ensure `quote` field contains the actual passage text. `EvidenceView.quote` is `Option<String>` — the query selects `fe.quote` directly from `fact_evidence`.
- [x] Ensure `document_id` is a UUID. `EvidenceView.document_id` is `Uuid`. Contract test asserts `is_string()`.
- [x] Write integration test: `aletheia_contract_tests.rs` — seeds entity+fact+evidence, calls evidence endpoint, asserts `{ "evidence": [...] }` envelope with quote, document_id, chunk_id.

### Search Contract

- [x] Verify `POST /api/v1/kbs/{kb}/search` accepts `{ "q": string, "top_k": usize }`. Confirmed in `search_routes.rs:14-23` — `SearchReq { q, top_k, as_of }`.
- [x] Verify response shape `{ "results": [...] }`. Confirmed in `search_routes.rs:40`. Contract tests on both sides.
- [x] Write integration test: `aletheia_contract_tests.rs` — seeds chunk, indexes in BM25, calls search endpoint, asserts `{ "results": [...] }` envelope with text and document_id.

### Service Account

- [x] Create a dedicated Aletheia service account: documented in `docs/aletheia-integration.md`. Non-admin user with Viewer role. Registration flow + required permissions documented.
- [x] Test Viewer role permissions boundary: `viewer_role_permissions_boundary` in `tests/core_api.rs` — Viewer can access entity detail and search (200), but is rejected on create entity and modify ontology (403).
- [x] Document the service account setup in a `docs/aletheia-integration.md` guide.

### Error Responses

- [x] Verify 404 response on non-existent entity: `entity_detail` returns `AppError::NotFound` → 404. Aletheia's error handler now maps 404 to a clear "entity not found in Utopia" message.
- [x] Verify 404 response on non-existent fact: `fact_evidence` returns empty array (not 404) for unknown facts. Aletheia handles this gracefully.
- [x] Document error response body format: Aletheia logs `{ status, body }` and does not parse error bodies. Utopia uses `AppError` which serializes as `{ "error": { "code": "...", "message": "..." } }` for validation errors and `"Unauthorized"` / `"Not Found"` for auth/404.

### Rate Limiting

- [x] Document rate limits for bulk entity resolution: `docs/rate-limiting.md` — documents current behavior (no rate limits), recommends bulk endpoint as ideal solution, notes Aletheia's new `resolve-batch` endpoint.

### Contract Stability

- [x] Add a CI contract test: `aletheia_contract_tests` module in `utopia-core/src/models.rs` serializes `GraphNode`, `EntityFact`, `EvidenceView` and asserts all fields Aletheia requires are present with correct types. Runs via `cargo test --workspace` in CI (step labeled "includes Aletheia contract tests").
- [x] Document which fields are part of the stable Aletheia contract in `docs/aletheia-contract.md` — lists all endpoints, fields, types, nullability, and which Aletheia serde model consumes each.
- [ ] Add `#[non_exhaustive]` to all Aletheia-facing response types (`GraphNode`, `EntityFact`, `EvidenceView`): prevents adding a required field without realizing it's a contract break. New fields must always be `Option<T>` or have serde defaults.

## Aletheia Integration — Deployment

- [x] Docker-compose: `docker-compose.aletheia.yml` overlay adds Aletheia service + its own Postgres. Wires `ALETHEIA_UTOPIA_BASE_URL=http://app:1516`, service account credentials, and Aletheia's DB URL.
- [x] Document full stack startup sequence: documented in `docs/aletheia-integration.md` — Postgres → Utopia → Aletheia-DB → Aletheia → register service account → grant Viewer → verify `/health`.
- [x] Kubernetes manifests: `k8s/aletheia-network-policy.yaml` (NetworkPolicy allowing Aletheia→Utopia on port 1516) + `k8s/aletheia-values.yaml` (Helm values for gitops-templates app-base chart).
- [x] Add health/readiness dependency: Aletheia's `/ready` now checks both DB and Utopia connectivity. Returns 503 if Utopia is unreachable or auth fails. Aletheia should not receive traffic until Utopia is healthy.

## Aletheia Integration — Operational Safety

- [x] Add Aletheia-contract-breaking CI label: `.github/workflows/aletheia-contract-label.yml` auto-labels PRs touching `models.rs`, `auth.rs`, `graph_routes.rs`, `search_routes.rs` with `aletheia-contract` and posts a compatibility warning comment.
- [x] Add webhook or notification when Utopia deploys a new version: documented in `docs/aletheia-integration.md` § "How to stay informed" — GitHub release notifications, `aletheia-contract` CI label subscription, and container SHA pinning strategy.

## Test Coverage Gaps

- [x] Add HTTP-level integration tests for core API routes: `tests/core_api.rs` — 6 tests covering register/login, wrong password, protected route, search entities, entity detail, search results.
- [x] Add tests for `utopia-extract` crate: `tests/extraction.rs` — JSON block parsing, entity/fact field parsing, attribute facts, deduplication, truncated JSON repair, adjudication parsing, time parsing, value normalization.
- [x] Add tests for `utopia-search` crate: `tests/search.rs` — indexing, BM25 search, KB scoping, reindex idempotency, deletion, limit cap, punctuation query, RRF fusion, DocsIndex.
- [x] Add frontend tests: vitest + @testing-library/react setup in `web/`. Smoke tests for Login and Search pages in `web/tests/`. Run with `pnpm test`.
- [x] Test Viewer role permissions boundary: covered by `viewer_role_permissions_boundary` in `tests/core_api.rs` (see Service Account section above).
- [ ] Test concurrent KB access via Aletheia's bulk resolve: two KBs referencing the same entity — verify no data leakage across KB boundaries (entity facts from KB-A must not appear in KB-B's results). Requires `aletheia_contract_tests.rs` expansion with two KBs.

## CI Hardening

- [ ] Add `cargo audit` step to CI `backend` job: Aletheia CI already runs `cargo audit` — Utopia should match. Install `cargo-audit` and run after build.
- [ ] Run `aletheia_contract_tests.rs` (server-level) in the `migrations` CI job: the `backend` job has no DB, so the HTTP-level Aletheia contract tests (`fact_evidence_endpoint_returns_expected_envelope`, `search_endpoint_returns_expected_envelope`) silently skip. Add them to the `migrations` job which has Postgres.
- [ ] Add `pnpm test` step to CI `web` job: frontend tests exist (`web/tests/`) but CI only runs `pnpm build` — tests are never executed in CI.

## Cross-Service Integration

- [ ] Automate service account provisioning in docker-compose: add an init container or entrypoint script to `docker-compose.aletheia.yml` that registers the Aletheia service account and grants Viewer role, removing the manual curl steps from first-time setup.
- [ ] Pin Aletheia container image to a specific SHA in `docker-compose.aletheia.yml` and `k8s/aletheia-values.yaml`: currently uses `ghcr.io/luukjuh123/aletheia:latest` — should pin to a tested release tag to prevent silent breakage on Utopia deploy.
- [ ] Add contract version header: Utopia should return `X-Utopia-Contract-Version: 1` on all Aletheia-consumed endpoints. Aletheia can log/alert when the version changes, enabling proactive compatibility checks before failures hit production.
