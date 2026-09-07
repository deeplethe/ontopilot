//! Integration tests validating the API contract Aletheia depends on.
//!
//! These tests hit the real Axum router against a real database. They are
//! skipped when `UTOPIA_DATABASE_URL` is not set (same gate as all other
//! store-backed tests). Set `UTOPIA_TEST_REQUIRE_DB=1` to turn a skip into
//! a failure, which is what CI does in the connected-database job.
//!
//! Included from `main.rs`:
//!   #[cfg(test)]
//!   #[path = "aletheia_contract_tests.rs"]
//!   mod aletheia_contract_tests;

use axum::http::{Request, StatusCode};
use axum::body::Body;
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt as _;
use uuid::Uuid;

use crate::state::AppState;
use utopia_core::config::AppConfig;
use utopia_search::SearchIndex;

// ---------------------------------------------------------------------------
// Shared setup helpers
// ---------------------------------------------------------------------------

/// Seed the minimal DB rows shared by both contract tests:
/// org → workspace → KB, a test user (admin = true so access always passes).
/// Returns (pool, state, jwt_token, kb_id, org_id).
async fn setup(
    url: &str,
) -> anyhow::Result<(PgPool, AppState, String, Uuid, Uuid)> {
    let pool = PgPool::connect(url).await?;

    let org_id = Uuid::now_v7();
    let ws_id = Uuid::now_v7();
    let kb_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();

    let suffix = org_id.to_string();
    sqlx::query("INSERT INTO organizations (id, name) VALUES ($1, $2)")
        .bind(org_id)
        .bind(format!("aletheia-contract-{suffix}"))
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO workspaces (id, org_id, name) VALUES ($1, $2, $3)")
        .bind(ws_id)
        .bind(org_id)
        .bind("aletheia-contract")
        .execute(&pool)
        .await?;
    sqlx::query(
        "INSERT INTO knowledge_bases (id, workspace_id, name) VALUES ($1, $2, $3)",
    )
    .bind(kb_id)
    .bind(ws_id)
    .bind("aletheia-contract")
    .execute(&pool)
    .await?;
    // Admin user — skips all KB access matrix checks.
    sqlx::query(
        "INSERT INTO users (id, org_id, email, password_hash, display_name, is_admin)
         VALUES ($1, $2, $3, 'test-hash', 'Test', TRUE)",
    )
    .bind(user_id)
    .bind(org_id)
    .bind(format!("aletheia-test-{suffix}@example.com"))
    .execute(&pool)
    .await?;

    let dir = std::env::temp_dir().join(format!("utopia-aletheia-{org_id}"));
    let cfg = AppConfig {
        data_dir: dir.to_string_lossy().into_owned(),
        ..Default::default()
    };
    let search = Arc::new(SearchIndex::open(&dir.join("search"))?);
    let state = AppState::new(pool.clone(), &cfg, search, "aletheia-test-secret".into());

    let token = crate::auth::issue_token(&state, user_id)?;

    Ok((pool, state, token, kb_id, org_id))
}

async fn cleanup(pool: &PgPool, org_id: Uuid) {
    let _ = sqlx::query("DELETE FROM organizations WHERE id = $1")
        .bind(org_id)
        .execute(pool)
        .await;
}

// ---------------------------------------------------------------------------
// Test 1 — GET /api/v1/kbs/{kb}/facts/{fact}/evidence
// ---------------------------------------------------------------------------

/// Aletheia calls this endpoint to display the source sentences behind a fact.
/// Contract: response is `{ "evidence": [...] }` where each item has
///   `quote` (string | null), `document_id` (UUID string), `chunk_id` (UUID string).
#[tokio::test]
async fn fact_evidence_endpoint_returns_expected_envelope() -> anyhow::Result<()> {
    let Some(url) = utopia_store::test_db::url() else {
        return Ok(());
    };

    let (pool, state, token, kb_id, org_id) = setup(&url).await?;

    let run = async {
        // ---- seed: source, document, chunk, entity type, entity, fact, evidence ----
        let src_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO sources (id, kb_id, name) VALUES ($1, $2, 'aletheia-contract-src')",
        )
        .bind(src_id)
        .bind(kb_id)
        .execute(&pool)
        .await?;

        let doc_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO documents (id, kb_id, source_id, filename, sha256, status)
             VALUES ($1, $2, $3, 'evidence-test.md', 'aletheia-evidence', 'ready')",
        )
        .bind(doc_id)
        .bind(kb_id)
        .bind(src_id)
        .execute(&pool)
        .await?;

        let chunk_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO chunks (id, kb_id, document_id, seq, text)
             VALUES ($1, $2, $3, 0, 'Paris is the capital of France.')",
        )
        .bind(chunk_id)
        .bind(kb_id)
        .bind(doc_id)
        .execute(&pool)
        .await?;

        let etype_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO entity_types (id, kb_id, key, label) VALUES ($1, $2, 'place', 'Place')",
        )
        .bind(etype_id)
        .bind(kb_id)
        .execute(&pool)
        .await?;

        let entity_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO entities (id, kb_id, type_id, canonical_name)
             VALUES ($1, $2, $3, 'Paris')",
        )
        .bind(entity_id)
        .bind(kb_id)
        .bind(etype_id)
        .execute(&pool)
        .await?;

        let fact_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO facts (id, kb_id, subject_id, confidence)
             VALUES ($1, $2, $3, 0.9)",
        )
        .bind(fact_id)
        .bind(kb_id)
        .bind(entity_id)
        .execute(&pool)
        .await?;

        sqlx::query(
            "INSERT INTO fact_evidence
             (fact_id, chunk_id, quote, proposed_predicate, document_id, doc_version)
             VALUES ($1, $2, 'Paris is the capital of France.', 'capital of', $3, 1)",
        )
        .bind(fact_id)
        .bind(chunk_id)
        .bind(doc_id)
        .execute(&pool)
        .await?;

        // ---- HTTP call ----
        let cfg = AppConfig {
            data_dir: std::env::temp_dir()
                .join(format!("utopia-aletheia-{org_id}"))
                .to_string_lossy()
                .into_owned(),
            ..Default::default()
        };
        let app = crate::api::router(state.clone(), &cfg);

        let uri = format!("/api/v1/kbs/{kb_id}/facts/{fact_id}/evidence");
        let req = Request::builder()
            .method("GET")
            .uri(&uri)
            .header("Authorization", format!("Bearer {token}"))
            .body(Body::empty())?;

        let resp = app.oneshot(req).await?;
        assert_eq!(resp.status(), StatusCode::OK, "evidence endpoint must return 200");

        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await?;
        let body: serde_json::Value = serde_json::from_slice(&bytes)?;

        // ---- assert envelope ----
        assert!(
            body.get("evidence").is_some(),
            "response must have top-level 'evidence' key; got: {body}"
        );
        let evidence = body["evidence"].as_array().expect("'evidence' must be an array");
        assert_eq!(evidence.len(), 1, "seeded exactly one evidence row");

        let item = &evidence[0];
        // Aletheia requires these three fields on every item:
        assert!(
            item.get("quote").is_some(),
            "evidence item must have 'quote' key"
        );
        assert!(
            item.get("document_id").is_some(),
            "evidence item must have 'document_id' key"
        );
        assert!(
            item.get("chunk_id").is_some(),
            "evidence item must have 'chunk_id' key"
        );

        // `quote` must be a string or null (Option<String>)
        let quote = &item["quote"];
        assert!(
            quote.is_string() || quote.is_null(),
            "'quote' must be a string or null, got: {quote}"
        );
        // `document_id` must be a UUID string
        let doc_str = item["document_id"]
            .as_str()
            .expect("'document_id' must be a string");
        doc_str
            .parse::<Uuid>()
            .expect("'document_id' must be a valid UUID string");
        // `chunk_id` must be a UUID string
        let chunk_str = item["chunk_id"]
            .as_str()
            .expect("'chunk_id' must be a string");
        chunk_str
            .parse::<Uuid>()
            .expect("'chunk_id' must be a valid UUID string");

        anyhow::Ok(())
    }
    .await;

    cleanup(&pool, org_id).await;
    run
}

// ---------------------------------------------------------------------------
// Test 2 — POST /api/v1/kbs/{kb}/search
// ---------------------------------------------------------------------------

/// Aletheia calls this endpoint to retrieve semantically relevant chunks.
/// Contract: response is `{ "results": [...] }` where each item has
///   `text` (string) and `document_id` (UUID string).
#[tokio::test]
async fn search_endpoint_returns_expected_envelope() -> anyhow::Result<()> {
    let Some(url) = utopia_store::test_db::url() else {
        return Ok(());
    };

    let (pool, state, token, kb_id, org_id) = setup(&url).await?;

    let run = async {
        // ---- seed: source, document, chunk ----
        let src_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO sources (id, kb_id, name) VALUES ($1, $2, 'aletheia-search-src')",
        )
        .bind(src_id)
        .bind(kb_id)
        .execute(&pool)
        .await?;

        let doc_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO documents (id, kb_id, source_id, filename, sha256, status)
             VALUES ($1, $2, $3, 'search-test.md', 'aletheia-search', 'ready')",
        )
        .bind(doc_id)
        .bind(kb_id)
        .bind(src_id)
        .execute(&pool)
        .await?;

        let chunk_id = Uuid::now_v7();
        let chunk_text = "The Eiffel Tower is located in Paris.";
        sqlx::query(
            "INSERT INTO chunks (id, kb_id, document_id, seq, text)
             VALUES ($1, $2, $3, 0, $4)",
        )
        .bind(chunk_id)
        .bind(kb_id)
        .bind(doc_id)
        .bind(chunk_text)
        .execute(&pool)
        .await?;

        // Index the chunk in BM25 so the search can actually find it.
        state
            .search
            .reindex_document(
                &kb_id.to_string(),
                &doc_id.to_string(),
                &[(chunk_id.to_string(), chunk_text.to_string())],
            )
            .map_err(|e| anyhow::anyhow!("reindex_document failed: {e}"))?;

        // ---- HTTP call ----
        let cfg = AppConfig {
            data_dir: std::env::temp_dir()
                .join(format!("utopia-aletheia-{org_id}"))
                .to_string_lossy()
                .into_owned(),
            ..Default::default()
        };
        let app = crate::api::router(state.clone(), &cfg);

        let body_json = serde_json::json!({ "q": "Eiffel Tower Paris", "top_k": 5 });
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/kbs/{kb_id}/search"))
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .body(Body::from(body_json.to_string()))?;

        let resp = app.oneshot(req).await?;
        assert_eq!(resp.status(), StatusCode::OK, "search endpoint must return 200");

        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await?;
        let body: serde_json::Value = serde_json::from_slice(&bytes)?;

        // ---- assert envelope ----
        assert!(
            body.get("results").is_some(),
            "response must have top-level 'results' key; got: {body}"
        );
        let results = body["results"].as_array().expect("'results' must be an array");

        // The BM25 hit for the indexed chunk must appear.
        assert!(
            !results.is_empty(),
            "expected at least one result for 'Eiffel Tower Paris' in the indexed chunk"
        );

        // Every result must carry `text` and `document_id` — the fields
        // Aletheia's client parses.
        for (i, item) in results.iter().enumerate() {
            assert!(
                item.get("text").and_then(|v| v.as_str()).is_some(),
                "result[{i}] must have non-null string 'text'"
            );
            let doc_str = item
                .get("document_id")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| panic!("result[{i}] must have string 'document_id'"));
            doc_str
                .parse::<Uuid>()
                .unwrap_or_else(|_| panic!("result[{i}].document_id must be a valid UUID"));
        }

        anyhow::Ok(())
    }
    .await;

    cleanup(&pool, org_id).await;
    run
}
