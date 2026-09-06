//! 治理先读台账（0025），打在真库上。
//!
//! 要钉住的行为：
//!
//! - **先例只认人的**：同一对被人分开过两次算两条，机器自己合过的那条不算；
//!   这一对合过、这个名字对别的名字、这种类型对的习惯、涉及这个名字的撤回，
//!   四族各归各
//! - **闸门**：人分开过的对模型说合只建议；合过的对模型说合就动手；有撤回只建议；
//!   类型冲突不合；没有任何先例不合，但能分
//! - **队列先进先出**，队头带着同名或同实体的簇；写了建议的对不再进队列；
//!   人裁了一对，同簇的建议作废、那些对回到队列
//! - **回答**：一条建议只能答一次；开关关了的库不在定时扫描里
//!
//! 没有 `UTOPIA_DATABASE_URL` 时跳过而不是失败。自建自拆，绝不碰已有的库。
//! 台账只增不删，种下的几行留在库里（kb 已删，kb_id 悬空，本就允许）。

use chrono::{Duration, Utc};
use sqlx::PgPool;
use utopia_store::governance::{self, Gate, NewDecision};
use uuid::Uuid;

struct Seed {
    kb: Uuid,
    org: Uuid,
    user: Uuid,
    /// 张伟 1 对 张伟 2：人分开过两次
    zw12: Uuid,
    /// Apple 对 Apple Inc.：人合过
    apple: Uuid,
    /// Orion 对 Orion Labs：合过又撤回
    orion: Uuid,
    /// Mercury（公司）对 Mercury（人）：类型冲突
    mercury: Uuid,
    /// 张伟 2 对 张伟 3：与第一对同名同实体，进簇
    zw23: Uuid,
}

async fn seed(pool: &PgPool) -> anyhow::Result<Seed> {
    let (org, ws, kb) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
    let (person, company) = (Uuid::now_v7(), Uuid::now_v7());
    let user = Uuid::now_v7();
    sqlx::query("INSERT INTO organizations (id, name) VALUES ($1, 'governor-test')")
        .bind(org)
        .execute(pool)
        .await?;
    sqlx::query(
        "INSERT INTO users (id, org_id, email, display_name, password_hash)
         VALUES ($1, $2, $3, 'Governor Tester', 'x')",
    )
    .bind(user)
    .bind(org)
    .bind(format!("governor-{}@test.local", user.simple()))
    .execute(pool)
    .await?;
    sqlx::query("INSERT INTO workspaces (id, org_id, name) VALUES ($1, $2, 'governor-test')")
        .bind(ws)
        .bind(org)
        .execute(pool)
        .await?;
    sqlx::query(
        "INSERT INTO knowledge_bases (id, workspace_id, name, governance)
         VALUES ($1, $2, 'governor-test', TRUE)",
    )
    .bind(kb)
    .bind(ws)
    .execute(pool)
    .await?;
    for (id, key, label) in [
        (person, "person", "Person"),
        (company, "org", "Organization"),
    ] {
        sqlx::query("INSERT INTO entity_types (id, kb_id, key, label) VALUES ($1, $2, $3, $4)")
            .bind(id)
            .bind(kb)
            .bind(key)
            .bind(label)
            .execute(pool)
            .await?;
    }
    let entity = |type_id: Uuid, name: &str| {
        let id = Uuid::now_v7();
        let name = name.to_string();
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO entities (id, kb_id, type_id, canonical_name) VALUES ($1, $2, $3, $4)",
            )
            .bind(id)
            .bind(kb)
            .bind(type_id)
            .bind(name)
            .execute(&pool)
            .await
            .map(|_| id)
        }
    };
    let zw1 = entity(person, "Zhang Wei").await?;
    let zw2 = entity(person, "Zhang Wei").await?;
    let zw3 = entity(person, "Zhang Wei").await?;
    let apple1 = entity(company, "Apple").await?;
    let apple2 = entity(company, "Apple Inc.").await?;
    let orion1 = entity(company, "Orion").await?;
    let orion2 = entity(company, "Orion Labs").await?;
    let mercury1 = entity(company, "Mercury").await?;
    let mercury2 = entity(person, "Mercury").await?;

    // 等人的对，created_at 隔开一分钟：先进先出要看得出顺序
    let t0 = Utc::now() - Duration::hours(1);
    let mut n = 0;
    let mut review = |l: Uuid, r: Uuid| {
        let id = Uuid::now_v7();
        let at = t0 + Duration::minutes(n);
        n += 1;
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO resolution_reviews
                    (id, kb_id, left_id, right_id, score, reason, stage, created_at)
                 VALUES ($1, $2, $3, $4, 1.0, 'namesake', 'human', $5)",
            )
            .bind(id)
            .bind(kb)
            .bind(l)
            .bind(r)
            .bind(at)
            .execute(&pool)
            .await
            .map(|_| id)
        }
    };
    let zw12 = review(zw1, zw2).await?;
    let apple = review(apple1, apple2).await?;
    let orion = review(orion1, orion2).await?;
    let mercury = review(mercury1, mercury2).await?;
    let zw23 = review(zw2, zw3).await?;

    // 台账：人的决定。actor 为空的那条是机器裁的，不算先例
    let ledger = |actor: Option<Uuid>, action: &str, detail: serde_json::Value| {
        let action = action.to_string();
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO audit_events (id, kb_id, actor_id, action, target_kind, detail)
                 VALUES ($1, $2, $3, $4, 'review', $5)",
            )
            .bind(Uuid::now_v7())
            .bind(kb)
            .bind(actor)
            .bind(action)
            .bind(detail)
            .execute(&pool)
            .await
            .map(|_| ())
        }
    };
    let pair = |l: &str, r: &str| serde_json::json!({ "left": l, "right": r, "score": 1.0 });
    ledger(Some(user), "review.keep", pair("Zhang Wei", "Zhang Wei")).await?;
    ledger(Some(user), "review.keep", pair("Zhang Wei", "Zhang Wei")).await?;
    ledger(None, "review.merge", pair("Zhang Wei", "Zhang Wei")).await?;
    ledger(Some(user), "review.merge", pair("Apple Inc.", "Apple")).await?;
    ledger(Some(user), "review.keep", pair("Apple", "Apple Records")).await?;
    ledger(
        Some(user),
        "merge.revert",
        serde_json::json!({ "source": "Orion", "target": "Orion Labs" }),
    )
    .await?;

    // 这种类型对的习惯：人裁过的 Person×Person 对，一合三分
    for status in ["merged", "kept", "kept", "kept"] {
        let (a, b) = (
            entity(person, "Someone").await?,
            entity(person, "Someone").await?,
        );
        sqlx::query(
            "INSERT INTO resolution_reviews
                (id, kb_id, left_id, right_id, score, stage, status, decided_at, decided_by)
             VALUES ($1, $2, $3, $4, 1.0, 'human', $5, now(), $6)",
        )
        .bind(Uuid::now_v7())
        .bind(kb)
        .bind(a)
        .bind(b)
        .bind(status)
        .bind(user)
        .execute(pool)
        .await?;
    }

    Ok(Seed {
        kb,
        org,
        user,
        zw12,
        apple,
        orion,
        mercury,
        zw23,
    })
}

async fn teardown(pool: &PgPool, s: &Seed) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM knowledge_bases WHERE id = $1")
        .bind(s.kb)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM organizations WHERE id = $1")
        .bind(s.org)
        .execute(pool)
        .await?;
    Ok(())
}

async fn run(pool: &PgPool, s: &Seed) -> anyhow::Result<()> {
    // 先进先出
    let q = governance::queue(pool, s.kb, 10).await?;
    let ids: Vec<Uuid> = q.iter().map(|i| i.id).collect();
    assert_eq!(ids, vec![s.zw12, s.apple, s.orion, s.mercury, s.zw23]);
    let by_id = |id: Uuid| q.iter().find(|i| i.id == id).cloned().unwrap();

    // 队头的簇：同名同实体的张伟 2 对 3
    let cluster = governance::cluster_of(pool, s.kb, &by_id(s.zw12), 11).await?;
    assert_eq!(
        cluster.iter().map(|i| i.id).collect::<Vec<_>>(),
        vec![s.zw23]
    );

    // 四族先例
    let p = governance::precedents_for(pool, s.kb, &by_id(s.zw12)).await?;
    assert_eq!(p.same_pair.len(), 2, "人分开过两次；机器合过的那条不算");
    assert!(p.same_pair.iter().all(|x| !x.merged()));
    assert!(p.same_name.is_empty());
    assert!(p.reverts.is_empty());
    let t = p.type_pair.clone().expect("两边都有类型");
    assert_eq!((t.merged, t.kept, t.reverted), (1, 3, 0));
    assert_eq!(governance::gate(Some(false), 0.9, false, &p), Gate::Apply);
    assert_eq!(
        governance::gate(Some(true), 0.99, false, &p),
        Gate::Propose,
        "人分开过，模型说合，只建议"
    );

    let p = governance::precedents_for(pool, s.kb, &by_id(s.apple)).await?;
    assert_eq!(p.same_pair.len(), 1, "左右对调也算这一对");
    assert!(p.same_pair[0].merged());
    assert_eq!(
        p.same_name.len(),
        1,
        "Apple 对 Apple Records 是这个名字对别的名字"
    );
    assert_eq!(governance::gate(Some(true), 0.9, false, &p), Gate::Apply);
    assert_eq!(governance::gate(Some(true), 0.7, false, &p), Gate::Propose);
    let lines = governance::render_lines(&p);
    assert!(lines[0].starts_with("this same pair was merged by a person on "));
    assert!(lines
        .iter()
        .any(|l| l.contains("\"Apple\" against \"Apple Records\" was kept apart")));

    let p = governance::precedents_for(pool, s.kb, &by_id(s.orion)).await?;
    assert_eq!(p.reverts.len(), 1);
    assert_eq!(governance::gate(Some(true), 0.99, false, &p), Gate::Propose);
    assert_eq!(
        governance::gate(Some(false), 0.99, false, &p),
        Gate::Propose
    );

    let m = by_id(s.mercury);
    let p = governance::precedents_for(pool, s.kb, &m).await?;
    assert!(p.type_pair.is_some());
    assert_eq!(governance::gate(Some(true), 0.99, true, &p), Gate::Propose);
    assert_eq!(governance::gate(Some(false), 0.9, true, &p), Gate::Apply);

    // 写了建议的对不再进队列
    let run_id = Uuid::now_v7();
    let d_apple = governance::record(
        pool,
        s.kb,
        NewDecision {
            run_id,
            target_id: s.apple,
            action: "merge",
            confidence: 0.7,
            reason: Some("the same pair was merged before"),
            precedents: governance::precedents_json(&p),
            status: "proposed",
            merge_id: None,
        },
    )
    .await?;
    let d_zw23 = governance::record(
        pool,
        s.kb,
        NewDecision {
            run_id,
            target_id: s.zw23,
            action: "keep",
            confidence: 0.6,
            reason: None,
            precedents: serde_json::json!([]),
            status: "proposed",
            merge_id: None,
        },
    )
    .await?;
    let ids: Vec<Uuid> = governance::queue(pool, s.kb, 10)
        .await?
        .iter()
        .map(|i| i.id)
        .collect();
    assert_eq!(ids, vec![s.zw12, s.orion, s.mercury]);
    assert_eq!(governance::count_proposed(pool, s.kb).await?, 2);
    assert_eq!(utopia_store::review::counts(pool, s.kb).await?.agent, 2);
    let listed = governance::list(pool, s.kb, 10, 0).await?;
    assert_eq!(listed.len(), 2, "最新的在前");
    assert_eq!(listed[0].id, d_zw23);
    assert_eq!(listed[1].left.as_deref(), Some("Apple"));
    assert_eq!(listed[1].right.as_deref(), Some("Apple Inc."));
    assert!(
        governance::record(
            pool,
            s.kb,
            NewDecision {
                run_id,
                target_id: s.apple,
                action: "keep",
                confidence: 0.5,
                reason: None,
                precedents: serde_json::json!([]),
                status: "proposed",
                merge_id: None,
            },
        )
        .await
        .is_err(),
        "一个目标同时只有一条开着的建议"
    );

    // 人裁了张伟 1 对 2：同簇的张伟 2 对 3 的建议作废、回到队列；Apple 的不受影响
    utopia_store::resolution::decide_review(pool, s.kb, s.zw12, "keep", s.user).await?;
    assert_eq!(governance::supersede_siblings(pool, s.kb, s.zw12).await?, 1);
    assert_eq!(
        governance::get(pool, s.kb, d_zw23).await?.status,
        "superseded"
    );
    assert_eq!(
        governance::get(pool, s.kb, d_apple).await?.status,
        "proposed"
    );
    let ids: Vec<Uuid> = governance::queue(pool, s.kb, 10)
        .await?
        .iter()
        .map(|i| i.id)
        .collect();
    assert_eq!(ids, vec![s.orion, s.mercury, s.zw23]);
    // 张伟 2 对 3 的同对先例还是种下的两条：decide_review 本身不写台账，台账由路由层记
    let p = governance::precedents_for(pool, s.kb, &by_id(s.zw23)).await?;
    assert_eq!(p.same_pair.len(), 2);

    // 回答一条建议：答过就不能再答
    governance::settle(pool, s.kb, d_apple, "accepted", s.user).await?;
    let d = governance::get(pool, s.kb, d_apple).await?;
    assert_eq!(d.status, "accepted");
    assert_eq!(d.decided_by_name.as_deref(), Some("Governor Tester"));
    assert!(
        governance::settle(pool, s.kb, d_apple, "overridden", s.user)
            .await
            .is_err()
    );

    // 定时扫描：开关开着且有没看过的对 → 在；关了 → 不在
    assert!(governance::due(pool).await?.contains(&s.kb));
    sqlx::query("UPDATE knowledge_bases SET governance = FALSE WHERE id = $1")
        .bind(s.kb)
        .execute(pool)
        .await?;
    assert!(!governance::due(pool).await?.contains(&s.kb));
    Ok(())
}

#[tokio::test]
async fn a_governor_reads_the_ledger() -> anyhow::Result<()> {
    let Some(url) = utopia_store::test_db::url() else {
        return Ok(());
    };
    let pool = PgPool::connect(&url).await?;
    let s = seed(&pool).await?;
    let outcome = run(&pool, &s).await;
    teardown(&pool, &s).await?;
    outcome
}
