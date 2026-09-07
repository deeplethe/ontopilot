//! Core API integration tests — HTTP layer against a real Postgres database.
//!
//! Skipped when `UTOPIA_DATABASE_URL` is not set. Set `UTOPIA_TEST_REQUIRE_DB=1`
//! to turn a skip into a failure (used by CI's connected-database job).
//!
//! Included from `main.rs`:
//!   #[cfg(test)]
//!   #[path = "../tests/core_api.rs"]
//!   mod core_api;
//!
//! This file lives under `tests/` for discoverability but is compiled as part
//! of the binary crate (no lib target), which is the established pattern in
//! this project (see `aletheia_contract_tests.rs`).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt as _;
use uuid::Uuid;

use crate::state::AppState;
use utopia_core::config::AppConfig;
use utopia_search::SearchIndex;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

macro_rules! require_db {
    () => {
        match utopia_store::test_db::url() {
            Some(url) => url,
            None => {
                eprintln!("skipping: UTOPIA_DATABASE_URL not set");
                return Ok(());
            }
        }
    };
}

/// Seed the minimal rows needed by most tests:
/// org → workspace → KB, one admin user.
/// Returns (pool, state, org_id, ws_id, kb_id, user_id).
async fn seed_org(
    url: &str,
    tag: &str,
) -> anyhow::Result<(PgPool, AppState, Uuid, Uuid, Uuid, Uuid)> {
    let pool = PgPool::connect(url).await?;

    let org_id = Uuid::now_v7();
    let ws_id = Uuid::now_v7();
    let kb_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let suffix = org_id.simple().to_string();

    sqlx::query("INSERT INTO organizations (id, name) VALUES ($1, $2)")
        .bind(org_id)
        .bind(format!("core-api-test-{tag}-{suffix}"))
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO workspaces (id, org_id, name) VALUES ($1, $2, $3)")
        .bind(ws_id)
        .bind(org_id)
        .bind(format!("core-api-ws-{tag}"))
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO knowledge_bases (id, workspace_id, name) VALUES ($1, $2, $3)")
        .bind(kb_id)
        .bind(ws_id)
        .bind(format!("core-api-kb-{tag}"))
        .execute(&pool)
        .await?;
    // is_admin = true so all KB access-matrix checks pass.
    sqlx::query(
        "INSERT INTO users (id, org_id, email, password_hash, display_name, is_admin)
         VALUES ($1, $2, $3, 'not-a-real-hash', 'Test', TRUE)",
    )
    .bind(user_id)
    .bind(org_id)
    .bind(format!("core-api-{tag}-{suffix}@example.com"))
    .execute(&pool)
    .await?;

    let dir = std::env::temp_dir().join(format!("utopia-core-api-{org_id}"));
    let cfg = AppConfig {
        data_dir: dir.to_string_lossy().into_owned(),
        ..Default::default()
    };
    let search = Arc::new(SearchIndex::open(&dir.join("search"))?);
    let state = AppState::new(pool.clone(), &cfg, search, "core-api-test-secret".into());

    Ok((pool, state, org_id, ws_id, kb_id, user_id))
}

/// Build an AppConfig that matches a given state's data_dir.
fn cfg_for(org_id: Uuid) -> AppConfig {
    AppConfig {
        data_dir: std::env::temp_dir()
            .join(format!("utopia-core-api-{org_id}"))
            .to_string_lossy()
            .into_owned(),
        ..Default::default()
    }
}

async fn delete_org(pool: &PgPool, org_id: Uuid) {
    let _ = sqlx::query("DELETE FROM organizations WHERE id = $1")
        .bind(org_id)
        .execute(pool)
        .await;
}

// ---------------------------------------------------------------------------
// Auth flow tests
// ---------------------------------------------------------------------------

/// POST /auth/register then POST /auth/login → 200, JWT token returned.
#[tokio::test]
async fn register_and_login() -> anyhow::Result<()> {
    let url = require_db!();
    // Use the store's accounts::register path via the HTTP router so that the
    // first-user bootstrap logic runs end-to-end.  We start from a fresh pool
    // with no org, which means we are the first user.
    let pool = PgPool::connect(&url).await?;
    let org_marker = Uuid::now_v7(); // unique suffix so this test's org doesn't collide
    let dir = std::env::temp_dir().join(format!("utopia-core-api-reg-{org_marker}"));
    let cfg = AppConfig {
        data_dir: dir.to_string_lossy().into_owned(),
        open_registration: true,
        ..Default::default()
    };
    let search = Arc::new(SearchIndex::open(&dir.join("search"))?);
    let state = AppState::new(pool.clone(), &cfg, search, "reg-test-secret".into());
    let app = crate::api::router(state, &cfg);

    let email = format!("register-{org_marker}@core-api-test.example");
    let password = "correct-horse-battery";

    // --- register ---
    let reg_body = serde_json::json!({
        "email": email,
        "password": password,
        "display_name": "Test User"
    });
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/register")
        .header("Content-Type", "application/json")
        .body(Body::from(reg_body.to_string()))?;

    let resp = app.clone().oneshot(req).await?;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "register must return 200"
    );
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await?;
    let body: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert!(
        body.get("token").and_then(|v| v.as_str()).is_some(),
        "register response must include a JWT token; got: {body}"
    );
    assert!(
        body.get("user").is_some(),
        "register response must include a user object; got: {body}"
    );

    // --- login ---
    let login_body = serde_json::json!({ "email": email, "password": password });
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header("Content-Type", "application/json")
        .body(Body::from(login_body.to_string()))?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK, "login must return 200");
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await?;
    let body: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert!(
        body.get("token").and_then(|v| v.as_str()).is_some(),
        "login response must include a JWT token; got: {body}"
    );

    // Teardown: delete the org that register created.
    let _ = sqlx::query(
        "DELETE FROM organizations WHERE id = (
             SELECT org_id FROM users WHERE email = $1 LIMIT 1)",
    )
    .bind(&email)
    .execute(&pool)
    .await;

    Ok(())
}

/// POST /auth/login with wrong password → 401.
#[tokio::test]
async fn login_wrong_password() -> anyhow::Result<()> {
    let url = require_db!();
    let (pool, state, org_id, _ws_id, _kb_id, _user_id) = seed_org(&url, "login-wrong").await?;
    let cfg = cfg_for(org_id);
    let app = crate::api::router(state, &cfg);

    // Seed a user with a known real argon2 hash for "correct-horse".
    let email = format!("login-wrong-{org_id}@example.com");
    let hash = crate::auth::hash_password("correct-horse")?;
    sqlx::query(
        "INSERT INTO users (id, org_id, email, password_hash, display_name)
         VALUES ($1, $2, $3, $4, 'Wrong Pw')",
    )
    .bind(Uuid::now_v7())
    .bind(org_id)
    .bind(&email)
    .bind(&hash)
    .execute(&pool)
    .await?;

    let body = serde_json::json!({ "email": email, "password": "wrong-password" });
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))?;

    let resp = app.oneshot(req).await?;
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "wrong password must return 401"
    );

    delete_org(&pool, org_id).await;
    Ok(())
}

/// GET /auth/me without any auth header → 401.
#[tokio::test]
async fn protected_route_without_auth() -> anyhow::Result<()> {
    let url = require_db!();
    let (pool, state, org_id, _ws_id, _kb_id, _user_id) =
        seed_org(&url, "no-auth").await?;
    let cfg = cfg_for(org_id);
    let app = crate::api::router(state, &cfg);

    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/auth/me")
        .body(Body::empty())?;

    let resp = app.oneshot(req).await?;
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "/auth/me without credentials must return 401"
    );

    delete_org(&pool, org_id).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// Entity CRUD tests (store-layer, Option A)
// ---------------------------------------------------------------------------

/// Search for entities by name returns results when seeded.
#[tokio::test]
async fn search_entities() -> anyhow::Result<()> {
    let url = require_db!();
    let (pool, state, org_id, _ws_id, kb_id, user_id) =
        seed_org(&url, "search-ents").await?;
    let cfg = cfg_for(org_id);
    let token = crate::auth::issue_token(&state, user_id)?;
    let app = crate::api::router(state, &cfg);

    // Seed an entity type and two entities.
    let etype_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO entity_types (id, kb_id, key, label) VALUES ($1, $2, 'org', 'Organization')",
    )
    .bind(etype_id)
    .bind(kb_id)
    .execute(&pool)
    .await?;

    for name in ["Acme Corp", "Beta Ltd"] {
        sqlx::query(
            "INSERT INTO entities (id, kb_id, type_id, canonical_name) VALUES ($1, $2, $3, $4)",
        )
        .bind(Uuid::now_v7())
        .bind(kb_id)
        .bind(etype_id)
        .bind(name)
        .execute(&pool)
        .await?;
    }

    let uri = format!("/api/v1/kbs/{kb_id}/entities?q=Acme");
    let req = Request::builder()
        .method("GET")
        .uri(&uri)
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK, "search entities must return 200");

    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await?;
    let body: serde_json::Value = serde_json::from_slice(&bytes)?;

    let entities = body["entities"]
        .as_array()
        .expect("response must have 'entities' array");
    assert!(
        !entities.is_empty(),
        "expected at least one entity for query 'Acme'; got: {body}"
    );
    let names: Vec<&str> = entities
        .iter()
        .filter_map(|e| e["canonical_name"].as_str())
        .collect();
    assert!(
        names.iter().any(|n| n.contains("Acme")),
        "search result should contain 'Acme'; got names: {names:?}"
    );
    assert!(
        body.get("total").is_some(),
        "response must include 'total' field; got: {body}"
    );

    delete_org(&pool, org_id).await;
    Ok(())
}

/// GET /kbs/{id}/entities/{entity_id} returns entity with facts and expected keys.
#[tokio::test]
async fn entity_detail() -> anyhow::Result<()> {
    let url = require_db!();
    let (pool, state, org_id, _ws_id, kb_id, user_id) =
        seed_org(&url, "ent-detail").await?;
    let cfg = cfg_for(org_id);
    let token = crate::auth::issue_token(&state, user_id)?;
    let app = crate::api::router(state, &cfg);

    // Seed entity type, two entities, a relation type, and a fact between them.
    let etype_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO entity_types (id, kb_id, key, label) VALUES ($1, $2, 'place', 'Place')",
    )
    .bind(etype_id)
    .bind(kb_id)
    .execute(&pool)
    .await?;

    let subject_id = Uuid::now_v7();
    let object_id = Uuid::now_v7();
    for (id, name) in [(subject_id, "Paris"), (object_id, "France")] {
        sqlx::query(
            "INSERT INTO entities (id, kb_id, type_id, canonical_name) VALUES ($1, $2, $3, $4)",
        )
        .bind(id)
        .bind(kb_id)
        .bind(etype_id)
        .bind(name)
        .execute(&pool)
        .await?;
    }

    let pred_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO relation_types (id, kb_id, key, label) VALUES ($1, $2, 'located_in', 'located in')",
    )
    .bind(pred_id)
    .bind(kb_id)
    .execute(&pool)
    .await?;

    let fact_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO facts (id, kb_id, subject_id, predicate_id, object_id, confidence)
         VALUES ($1, $2, $3, $4, $5, 0.95)",
    )
    .bind(fact_id)
    .bind(kb_id)
    .bind(subject_id)
    .bind(pred_id)
    .bind(object_id)
    .execute(&pool)
    .await?;

    let uri = format!("/api/v1/kbs/{kb_id}/entities/{subject_id}");
    let req = Request::builder()
        .method("GET")
        .uri(&uri)
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK, "entity_detail must return 200");

    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await?;
    let body: serde_json::Value = serde_json::from_slice(&bytes)?;

    // The handler returns { entity, facts, derived, blocked, same_name }.
    assert!(
        body.get("entity").is_some(),
        "response must have 'entity' key; got: {body}"
    );
    assert!(
        body.get("facts").is_some(),
        "response must have 'facts' key; got: {body}"
    );
    assert!(
        body.get("derived").is_some(),
        "response must have 'derived' key; got: {body}"
    );

    let entity = &body["entity"];
    assert_eq!(
        entity["canonical_name"].as_str(),
        Some("Paris"),
        "entity name must match seeded value"
    );

    let facts = body["facts"].as_array().expect("'facts' must be an array");
    assert_eq!(facts.len(), 1, "expected exactly one fact");

    delete_org(&pool, org_id).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// Search test (HTTP layer)
// ---------------------------------------------------------------------------

/// POST /kbs/{id}/search returns results for an indexed chunk.
#[tokio::test]
async fn search_returns_results() -> anyhow::Result<()> {
    let url = require_db!();
    let (pool, state, org_id, _ws_id, kb_id, user_id) =
        seed_org(&url, "search-results").await?;
    let cfg = cfg_for(org_id);
    let token = crate::auth::issue_token(&state, user_id)?;

    // Seed source → document → chunk, then index the chunk in BM25.
    let src_id = Uuid::now_v7();
    sqlx::query("INSERT INTO sources (id, kb_id, name) VALUES ($1, $2, 'core-api-search-src')")
        .bind(src_id)
        .bind(kb_id)
        .execute(&pool)
        .await?;

    let doc_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO documents (id, kb_id, source_id, filename, sha256, status)
         VALUES ($1, $2, $3, 'core-api-search.md', 'core-api-sha', 'ready')",
    )
    .bind(doc_id)
    .bind(kb_id)
    .bind(src_id)
    .execute(&pool)
    .await?;

    let chunk_id = Uuid::now_v7();
    let chunk_text = "The quantum engine enables faster-than-light computation.";
    sqlx::query(
        "INSERT INTO chunks (id, kb_id, document_id, seq, text) VALUES ($1, $2, $3, 0, $4)",
    )
    .bind(chunk_id)
    .bind(kb_id)
    .bind(doc_id)
    .bind(chunk_text)
    .execute(&pool)
    .await?;

    // Write the chunk into the BM25 index so the search handler can find it.
    state
        .search
        .reindex_document(
            &kb_id.to_string(),
            &doc_id.to_string(),
            &[(chunk_id.to_string(), chunk_text.to_string())],
        )
        .map_err(|e| anyhow::anyhow!("reindex_document failed: {e}"))?;

    let app = crate::api::router(state, &cfg);

    let body_json = serde_json::json!({ "q": "quantum engine computation", "top_k": 5 });
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/kbs/{kb_id}/search"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(body_json.to_string()))?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK, "search must return 200");

    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await?;
    let body: serde_json::Value = serde_json::from_slice(&bytes)?;

    assert!(
        body.get("results").is_some(),
        "response must have 'results' key; got: {body}"
    );
    let results = body["results"]
        .as_array()
        .expect("'results' must be an array");
    assert!(
        !results.is_empty(),
        "expected at least one result for 'quantum engine computation'"
    );

    // Each result must carry `text` and `document_id`.
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

    delete_org(&pool, org_id).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// Viewer role permissions boundary (Aletheia service account contract)
// ---------------------------------------------------------------------------

/// A Viewer user can call entity detail, fact evidence, and search (the endpoints
/// Aletheia uses) but must be rejected on write endpoints (create entity, modify
/// ontology). This is the Aletheia service account's permission boundary.
#[tokio::test]
async fn viewer_role_permissions_boundary() -> anyhow::Result<()> {
    let url = require_db!();
    let (pool, state, org_id, _ws_id, kb_id, _admin_user_id) =
        seed_org(&url, "viewer-perms").await?;
    let cfg = cfg_for(org_id);

    // Create a non-admin user with Viewer role on the KB
    let viewer_id = Uuid::now_v7();
    let suffix = org_id.simple().to_string();
    sqlx::query(
        "INSERT INTO users (id, org_id, email, password_hash, display_name, is_admin)
         VALUES ($1, $2, $3, 'not-a-real-hash', 'Aletheia Service', FALSE)",
    )
    .bind(viewer_id)
    .bind(org_id)
    .bind(format!("viewer-{suffix}@example.com"))
    .execute(&pool)
    .await?;

    sqlx::query(
        "INSERT INTO kb_members (kb_id, user_id, role) VALUES ($1, $2, 'viewer')",
    )
    .bind(kb_id)
    .bind(viewer_id)
    .execute(&pool)
    .await?;

    let viewer_token = crate::auth::issue_token(&state, viewer_id)?;

    // Seed entity type + entity for the read tests
    let etype_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO entity_types (id, kb_id, key, label) VALUES ($1, $2, 'org', 'Org')",
    )
    .bind(etype_id)
    .bind(kb_id)
    .execute(&pool)
    .await?;

    let entity_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO entities (id, kb_id, type_id, canonical_name)
         VALUES ($1, $2, $3, 'ViewerTestEntity')",
    )
    .bind(entity_id)
    .bind(kb_id)
    .bind(etype_id)
    .execute(&pool)
    .await?;

    let app = crate::api::router(state, &cfg);

    // ---- Viewer CAN access entity detail ----
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/kbs/{kb_id}/entities/{entity_id}"))
        .header("Authorization", format!("Bearer {viewer_token}"))
        .body(Body::empty())?;
    let resp = app.clone().oneshot(req).await?;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "Viewer must be able to read entity detail"
    );

    // ---- Viewer CAN access search ----
    let body = serde_json::json!({"q": "test query", "top_k": 1});
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/kbs/{kb_id}/search"))
        .header("Authorization", format!("Bearer {viewer_token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))?;
    let resp = app.clone().oneshot(req).await?;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "Viewer must be able to search"
    );

    // ---- Viewer CANNOT create entities (requires Editor) ----
    let body = serde_json::json!({"canonical_name": "ShouldNotCreate", "type_id": etype_id});
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/kbs/{kb_id}/entities"))
        .header("Authorization", format!("Bearer {viewer_token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))?;
    let resp = app.clone().oneshot(req).await?;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "Viewer must NOT be able to create entities"
    );

    // ---- Viewer CANNOT modify ontology (requires Editor) ----
    let body = serde_json::json!({"key": "test_type", "label": "Test Type"});
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/kbs/{kb_id}/ontology/entity-types"))
        .header("Authorization", format!("Bearer {viewer_token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))?;
    let resp = app.clone().oneshot(req).await?;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "Viewer must NOT be able to modify ontology"
    );

    delete_org(&pool, org_id).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// Concurrent KB boundary test — no data leakage across KBs
// ---------------------------------------------------------------------------

/// Entities and facts in KB-A must not appear in KB-B responses when both are
/// accessed concurrently. Guards against cross-tenant data leakage.
#[tokio::test]
async fn concurrent_kb_access_no_data_leakage() -> anyhow::Result<()> {
    let url = require_db!();
    let (pool, state, org_id, ws_id, kb_a, user_id) =
        seed_org(&url, "kb-boundary").await?;
    let cfg = cfg_for(org_id);
    let token = crate::auth::issue_token(&state, user_id)?;

    // Create kb_b in the same workspace.
    let kb_b = Uuid::now_v7();
    sqlx::query("INSERT INTO knowledge_bases (id, workspace_id, name) VALUES ($1, $2, $3)")
        .bind(kb_b)
        .bind(ws_id)
        .bind("core-api-kb-boundary-b")
        .execute(&pool)
        .await?;

    // Seed a shared entity type (one per KB — entity_types are KB-scoped).
    let etype_a = Uuid::now_v7();
    let etype_b = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO entity_types (id, kb_id, key, label) VALUES ($1, $2, 'person', 'Person')",
    )
    .bind(etype_a)
    .bind(kb_a)
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO entity_types (id, kb_id, key, label) VALUES ($1, $2, 'person', 'Person')",
    )
    .bind(etype_b)
    .bind(kb_b)
    .execute(&pool)
    .await?;

    // Seed one entity in each KB.
    let entity_a = Uuid::now_v7();
    let entity_a2 = Uuid::now_v7(); // fact target in kb_a
    let entity_b = Uuid::now_v7();
    let entity_b2 = Uuid::now_v7(); // fact target in kb_b

    for (id, kb, etype, name) in [
        (entity_a, kb_a, etype_a, "Alice"),
        (entity_a2, kb_a, etype_a, "AliceTarget"),
        (entity_b, kb_b, etype_b, "Bob"),
        (entity_b2, kb_b, etype_b, "BobTarget"),
    ] {
        sqlx::query(
            "INSERT INTO entities (id, kb_id, type_id, canonical_name) VALUES ($1, $2, $3, $4)",
        )
        .bind(id)
        .bind(kb)
        .bind(etype)
        .bind(name)
        .execute(&pool)
        .await?;
    }

    // Seed relation types (one per KB).
    let pred_a = Uuid::now_v7();
    let pred_b = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO relation_types (id, kb_id, key, label) VALUES ($1, $2, 'knows', 'knows')",
    )
    .bind(pred_a)
    .bind(kb_a)
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO relation_types (id, kb_id, key, label) VALUES ($1, $2, 'works_with', 'works with')",
    )
    .bind(pred_b)
    .bind(kb_b)
    .execute(&pool)
    .await?;

    // Seed fact_a in kb_a only (Alice knows AliceTarget).
    let fact_a = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO facts (id, kb_id, subject_id, predicate_id, object_id, confidence)
         VALUES ($1, $2, $3, $4, $5, 0.9)",
    )
    .bind(fact_a)
    .bind(kb_a)
    .bind(entity_a)
    .bind(pred_a)
    .bind(entity_a2)
    .execute(&pool)
    .await?;

    // Seed fact_b in kb_b only (Bob works_with BobTarget).
    let fact_b = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO facts (id, kb_id, subject_id, predicate_id, object_id, confidence)
         VALUES ($1, $2, $3, $4, $5, 0.85)",
    )
    .bind(fact_b)
    .bind(kb_b)
    .bind(entity_b)
    .bind(pred_b)
    .bind(entity_b2)
    .execute(&pool)
    .await?;

    let app = crate::api::router(state, &cfg);

    // Concurrently GET entity_a from kb_a and entity_b from kb_b.
    let uri_a = format!("/api/v1/kbs/{kb_a}/entities/{entity_a}");
    let uri_b = format!("/api/v1/kbs/{kb_b}/entities/{entity_b}");

    let req_a = Request::builder()
        .method("GET")
        .uri(&uri_a)
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())?;
    let req_b = Request::builder()
        .method("GET")
        .uri(&uri_b)
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())?;

    let (resp_a, resp_b) = tokio::join!(
        app.clone().oneshot(req_a),
        app.clone().oneshot(req_b),
    );
    let resp_a = resp_a?;
    let resp_b = resp_b?;

    assert_eq!(resp_a.status(), StatusCode::OK, "kb_a entity detail must return 200");
    assert_eq!(resp_b.status(), StatusCode::OK, "kb_b entity detail must return 200");

    let bytes_a = axum::body::to_bytes(resp_a.into_body(), 1024 * 1024).await?;
    let bytes_b = axum::body::to_bytes(resp_b.into_body(), 1024 * 1024).await?;
    let body_a: serde_json::Value = serde_json::from_slice(&bytes_a)?;
    let body_b: serde_json::Value = serde_json::from_slice(&bytes_b)?;

    // kb_a response: has fact_a (knows), does NOT contain fact_b's predicate (works_with).
    let facts_a = body_a["facts"].as_array().expect("kb_a 'facts' must be array");
    assert_eq!(facts_a.len(), 1, "kb_a must have exactly one fact");
    let pred_key_a = facts_a[0]["predicate_key"].as_str().unwrap_or("");
    assert_eq!(pred_key_a, "knows", "kb_a fact must be 'knows'");
    let has_works_with_in_a = facts_a
        .iter()
        .any(|f| f["predicate_key"].as_str() == Some("works_with"));
    assert!(
        !has_works_with_in_a,
        "kb_a must NOT contain kb_b's 'works_with' fact; got: {body_a}"
    );

    // kb_b response: has fact_b (works_with), does NOT contain fact_a's predicate (knows).
    let facts_b = body_b["facts"].as_array().expect("kb_b 'facts' must be array");
    assert_eq!(facts_b.len(), 1, "kb_b must have exactly one fact");
    let pred_key_b = facts_b[0]["predicate_key"].as_str().unwrap_or("");
    assert_eq!(pred_key_b, "works_with", "kb_b fact must be 'works_with'");
    let has_knows_in_b = facts_b
        .iter()
        .any(|f| f["predicate_key"].as_str() == Some("knows"));
    assert!(
        !has_knows_in_b,
        "kb_b must NOT contain kb_a's 'knows' fact; got: {body_b}"
    );

    delete_org(&pool, org_id).await;
    Ok(())
}
