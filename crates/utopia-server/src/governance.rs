//! 治理任务（0025）：开关开着就把等人的重复对按先进先出过一遍，先读台账再裁。
//!
//! 队头带着它的簇进一次模型调用；过闸的自己动手（合并可撤、分开可再合），
//! 过不了的写成建议留给人。两簇之间看一眼开关，关掉就停在这里——「关闭后队列
//! 自动终止」就是那一行。模型缺席时任务成功结束、什么都不动：没有依据的裁决
//! 不如不裁，队列原地等。
//!
//! 与攒批裁决器（`adjudication`）的分工：开关关着，那一档照旧独自判灰区对；
//! 开着，抽取结束排的是这里，灰区对与等人的对都从这条队列走。这里不读也不写
//! 裁决缓存——缓存键里没有先例，而这里的答案随先例变。

use crate::llm_util;
use crate::state::AppState;
use std::collections::HashMap;
use utopia_core::models::ReviewItem;
use utopia_core::AppError;
use utopia_store::governance::{self, Gate, NewDecision, Precedents};
use uuid::Uuid;

/// 一次模型调用带几对：队头加它的簇
const BATCH_SIZE: i64 = 12;
/// 一个任务最多走几轮，之后再排一个接着走——让别的库的任务也轮得上
const MAX_ROUNDS: usize = 20;

pub async fn govern(state: &AppState, kb_id: Uuid) -> anyhow::Result<()> {
    let kb = utopia_store::kbs::get(&state.pool, kb_id).await?;
    if !kb.governance {
        return Ok(());
    }
    let settings = utopia_store::settings::get(&state.pool, kb.workspace_id).await?;
    let Some(client) = settings.as_ref().and_then(llm_util::chat_client) else {
        tracing::info!(%kb_id, "治理：没有配聊天模型，队列原地等");
        return Ok(());
    };
    let run_id = Uuid::now_v7();

    for _ in 0..MAX_ROUNDS {
        // 关闭后队列自动终止：每一簇之前看一眼
        if !utopia_store::kbs::get(&state.pool, kb_id).await?.governance {
            tracing::info!(%kb_id, "治理：开关已关，停在这里");
            return Ok(());
        }
        let Some(head) = governance::queue(&state.pool, kb_id, 1)
            .await?
            .into_iter()
            .next()
        else {
            return Ok(());
        };
        let mut items = vec![head];
        let siblings =
            governance::cluster_of(&state.pool, kb_id, &items[0], BATCH_SIZE - 1).await?;
        items.extend(siblings);

        let mut precedents = Vec::with_capacity(items.len());
        for item in &items {
            precedents.push(governance::precedents_for(&state.pool, kb_id, item).await?);
        }
        let pairs: Vec<utopia_extract::AdjudicationPair> = items
            .iter()
            .zip(&precedents)
            .map(|(item, p)| utopia_extract::AdjudicationPair {
                left: side(&item.left),
                right: side(&item.right),
                precedents: governance::render_lines(p),
            })
            .collect();
        let messages = utopia_extract::build_adjudication_messages(&pairs);
        let _permit = match settings.as_ref().map(|s| llm_util::acquire_chat(state, s)) {
            Some(f) => f.await,
            None => None,
        };
        // 调用/解析失败 → 任务按退避重试；重试耗尽后这些对留在队列里，人照样能裁
        let reply = client.chat(&messages).await?;
        let verdicts = utopia_extract::parse_adjudication(&reply)?;
        let by_i: HashMap<usize, &utopia_extract::AdjudicationVerdict> =
            verdicts.iter().map(|v| (v.i, v)).collect();

        for (idx, item) in items.iter().enumerate() {
            let (same, conf, why) = match by_i.get(&idx) {
                Some(v) => (
                    match v.verdict.as_str() {
                        "same" => Some(true),
                        "different" => Some(false),
                        _ => None,
                    },
                    v.confidence.unwrap_or(0.5).clamp(0.0, 1.0),
                    v.why.clone(),
                ),
                None => (None, 0.0, None),
            };
            settle(
                state,
                kb_id,
                run_id,
                item,
                &precedents[idx],
                same,
                conf,
                why.as_deref(),
            )
            .await?;
        }
        state.emit_review(kb_id);
    }

    // 轮数用完还有积压：再排一个，下一轮从队头接着走
    if !governance::queue(&state.pool, kb_id, 1).await?.is_empty() {
        utopia_store::jobs::enqueue_unless_queued(
            &state.pool,
            "govern",
            serde_json::json!({ "kb_id": kb_id }),
        )
        .await?;
    }
    Ok(())
}

fn side(s: &utopia_core::models::ReviewSide) -> utopia_extract::AdjudicationSide {
    utopia_extract::AdjudicationSide {
        name: s.name.clone(),
        type_label: s.type_label.clone().unwrap_or_else(|| "untyped".into()),
        facts: s.top_facts.clone(),
    }
}

/// 一对的判决落地：过闸就动手并记 applied，否则转人工并记 proposed
#[allow(clippy::too_many_arguments)]
async fn settle(
    state: &AppState,
    kb_id: Uuid,
    run_id: Uuid,
    item: &ReviewItem,
    p: &Precedents,
    same: Option<bool>,
    conf: f32,
    why: Option<&str>,
) -> anyhow::Result<()> {
    let pool = &state.pool;
    let types_conflict = matches!(
        (&item.left.type_label, &item.right.type_label),
        (Some(a), Some(b)) if a != b
    );
    let action = match same {
        Some(true) => "merge",
        Some(false) => "keep",
        None => "unsure",
    };
    let precedents = governance::precedents_json(p);
    let decision = |status: &'static str, merge_id: Option<Uuid>| NewDecision {
        run_id,
        target_id: item.id,
        action,
        confidence: conf,
        reason: why,
        precedents: precedents.clone(),
        status,
        merge_id,
    };

    match governance::gate(same, conf, types_conflict, p) {
        Gate::Apply if same == Some(true) => {
            let reason = format!("governed|{conf:.2}");
            // 同簇连锁：前一对合完，这一对的一侧可能已经并进了别人——合活着的那个
            let (l, r) = (
                utopia_store::resolution::survivor(pool, kb_id, item.left.id).await?,
                utopia_store::resolution::survivor(pool, kb_id, item.right.id).await?,
            );
            if l == r {
                // 两边已经是同一个实体：只剩把审核行关上
                utopia_store::resolution::close_review_auto(pool, item.id, "merged", &reason)
                    .await?;
                let id = governance::record(pool, kb_id, decision("applied", None)).await?;
                audit(state, kb_id, "review.merge", item, conf, id).await;
                return Ok(());
            }
            let (target, source) = utopia_store::resolution::merge_direction(pool, l, r).await?;
            match utopia_store::resolution::merge_entities(
                pool, kb_id, source, target, None, &reason,
            )
            .await
            {
                Ok(merge_id) => {
                    utopia_store::resolution::close_review_auto(pool, item.id, "merged", &reason)
                        .await?;
                    let id = governance::record(pool, kb_id, decision("applied", Some(merge_id)))
                        .await?;
                    audit(state, kb_id, "review.merge", item, conf, id).await;
                }
                // 同簇连锁合并已吞掉一方：留给人，并记一条 unsure 免得下一轮又撞上
                Err(AppError::Conflict(_)) | Err(AppError::NotFound) => {
                    utopia_store::resolution::escalate_review(
                        pool,
                        item.id,
                        "escalate_entity_changed",
                    )
                    .await?;
                    governance::record(
                        pool,
                        kb_id,
                        NewDecision {
                            action: "unsure",
                            reason: Some("one side changed while this cluster was being decided"),
                            ..decision("proposed", None)
                        },
                    )
                    .await?;
                }
                Err(e) => return Err(e.into()),
            }
        }
        Gate::Apply => {
            let reason = format!("governed|{conf:.2}");
            utopia_store::resolution::close_review_auto(pool, item.id, "kept", &reason).await?;
            let id = governance::record(pool, kb_id, decision("applied", None)).await?;
            audit(state, kb_id, "review.keep", item, conf, id).await;
        }
        Gate::Propose => {
            utopia_store::resolution::escalate_review(pool, item.id, "proposed").await?;
            governance::record(pool, kb_id, decision("proposed", None)).await?;
        }
    }
    Ok(())
}

/// 决策台账：actor 为空（机器），detail 里说是治理、置信度多少、对应哪条 agent 决定
async fn audit(
    state: &AppState,
    kb_id: Uuid,
    action: &str,
    item: &ReviewItem,
    conf: f32,
    id: Uuid,
) {
    let _ = utopia_store::audit::record_opt(
        &state.pool,
        Some(kb_id),
        None,
        action,
        "review",
        Some(item.id),
        serde_json::json!({
            "left": item.left.name, "right": item.right.name, "score": item.score,
            "confidence": conf, "via": "governor", "decision": id,
        }),
    )
    .await;
}

/// 人裁了一对之后：同名或同实体的对上的建议过时、那些对回到队列；开关开着就排一轮。
/// 人批量分开三对张伟，agent 顺着同一簇把剩下的照办——#428 要的自动处理就是这一步
pub async fn after_human_decision(state: &AppState, kb_id: Uuid, review_ids: &[Uuid]) {
    for &id in review_ids {
        if let Err(e) = governance::supersede_siblings(&state.pool, kb_id, id).await {
            tracing::warn!(%kb_id, review = %id, error = %e, "治理：建议作废失败");
        }
    }
    match utopia_store::kbs::get(&state.pool, kb_id).await {
        Ok(kb) if kb.governance => {
            if let Err(e) = utopia_store::jobs::enqueue_unless_queued(
                &state.pool,
                "govern",
                serde_json::json!({ "kb_id": kb_id }),
            )
            .await
            {
                tracing::warn!(%kb_id, error = %e, "治理任务入队失败");
            }
        }
        _ => {}
    }
}
