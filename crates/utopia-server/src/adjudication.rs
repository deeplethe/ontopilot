//! 实体消解攒批裁决任务：消费审核队列中 stage=adjudicating 的灰区对。
//! 先查裁决缓存，缓存未命中的攒成一批（一次 LLM 调用裁多对）；
//! 高置信 same → 自动合并（可回滚），高置信 different → 自动保持分开，
//! 其余转人工。未配模型时全部转人工——本任务失败或缺席都不影响抽取与查询。

use crate::llm_util;
use crate::state::AppState;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use utopia_core::models::ReviewItem;
use utopia_core::AppError;
use utopia_store::governance as gov;
use uuid::Uuid;

const BATCH_SIZE: i64 = 12;
const AUTO_CONF: f32 = 0.8;
const MAX_ROUNDS: usize = 20;
/// 每一百对里有几对**即使裁决器有把握也交给人**（0026）。没有这一份，人裁过的
/// 语料会收缩成只剩难题：既不再代表一般的判法，也没有样本能量出机器与人的
/// 一致率。按 id 取样而不是掷骰子：同一对每次跑到的答案一样，才好复现
const HUMAN_SAMPLE_PCT: u128 = 10;

/// 缓存键：类型 + 双方名字 + 事实摘要 + 先例（与实体 id 无关——重传文档不重复付费）。
/// **先例也进键**：答案随先例变（0025 说的正是这个，治理那一路因此干脆不用缓存）；
/// 人又裁了一笔，键就变，旧答案自然作废，不必去清
fn pair_key(item: &ReviewItem, precedents: &[String]) -> String {
    let side = |s: &utopia_core::models::ReviewSide| {
        format!(
            "{}|{}|{}",
            s.name.to_lowercase(),
            // 没判出类型的一侧照样要能缓存（0009）
            s.type_label.as_deref().unwrap_or("untyped"),
            s.top_facts.join(";")
        )
    };
    let mut sides = [side(&item.left), side(&item.right)];
    sides.sort();
    let digest =
        Sha256::digest(format!("{}##{}", sides.join("##"), precedents.join("\n")).as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// 这一对是不是抽给人看的那一份（见 HUMAN_SAMPLE_PCT）
fn sampled_for_a_person(item: &ReviewItem) -> bool {
    item.id.as_u128() % 100 < HUMAN_SAMPLE_PCT
}

pub async fn adjudicate_entities(state: &AppState, kb_id: Uuid) -> anyhow::Result<()> {
    let kb = utopia_store::kbs::get(&state.pool, kb_id).await?;
    let settings = utopia_store::settings::get(&state.pool, kb.workspace_id).await?;
    let client = settings.as_ref().and_then(llm_util::chat_client);
    let model = settings
        .as_ref()
        .and_then(|s| s.chat_model.clone())
        .unwrap_or_default();

    let Some(client) = client else {
        // 无模型可用：全部转人工，任务本身成功结束
        let items =
            utopia_store::resolution::pending_adjudications(&state.pool, kb_id, 500).await?;
        for item in items {
            utopia_store::resolution::escalate_review(&state.pool, item.id, "escalate_no_model")
                .await?;
        }
        state.emit_review(kb_id);
        return Ok(());
    };

    for _ in 0..MAX_ROUNDS {
        let items =
            utopia_store::resolution::pending_adjudications(&state.pool, kb_id, BATCH_SIZE).await?;
        if items.is_empty() {
            break;
        }

        // 第一层：裁决缓存。先例（人在这个库里对这些名字做过什么，连同他们写的理由）
        // 先取出来：它既进提示词也进缓存键
        let mut to_ask: Vec<(ReviewItem, String, Vec<String>)> = Vec::new();
        for item in items {
            let precedents =
                gov::render_lines(&gov::precedents_for(&state.pool, kb_id, &item).await?);
            let key = pair_key(&item, &precedents);
            match utopia_store::resolution::get_verdict(&state.pool, kb_id, &key).await? {
                Some((same, conf)) => {
                    apply_verdict(state, kb_id, &item, same, conf, "cached", None).await?
                }
                None => to_ask.push((item, key, precedents)),
            }
        }
        if to_ask.is_empty() {
            continue;
        }

        // 第二层：攒批 LLM 裁决
        let pairs: Vec<utopia_extract::AdjudicationPair> = to_ask
            .iter()
            .map(|(item, _, precedents)| utopia_extract::AdjudicationPair {
                left: utopia_extract::AdjudicationSide {
                    name: item.left.name.clone(),
                    type_label: item
                        .left
                        .type_label
                        .clone()
                        .unwrap_or_else(|| "untyped".into()),
                    facts: item.left.top_facts.clone(),
                },
                right: utopia_extract::AdjudicationSide {
                    name: item.right.name.clone(),
                    type_label: item
                        .right
                        .type_label
                        .clone()
                        .unwrap_or_else(|| "untyped".into()),
                    facts: item.right.top_facts.clone(),
                },
                precedents: precedents.clone(),
            })
            .collect();
        let messages = utopia_extract::build_adjudication_messages(&pairs);
        // 调用/解析失败 → 任务按退避重试；重试耗尽后行停留在队列里，人工仍可定夺
        let _permit = settings.as_ref().map(|s| llm_util::acquire_chat(state, s));
        let _permit = match _permit {
            Some(f) => f.await,
            None => None,
        };
        let reply = client.chat(&messages).await?;
        let verdicts = utopia_extract::parse_adjudication(&reply)?;
        let by_i: HashMap<usize, &utopia_extract::AdjudicationVerdict> =
            verdicts.iter().map(|v| (v.i, v)).collect();

        for (idx, (item, key, _)) in to_ask.iter().enumerate() {
            match by_i.get(&idx) {
                Some(v) => {
                    let same = match v.verdict.as_str() {
                        "same" => Some(true),
                        "different" => Some(false),
                        _ => None,
                    };
                    let conf = v.confidence.unwrap_or(0.5).clamp(0.0, 1.0);
                    utopia_store::resolution::put_verdict(
                        &state.pool,
                        kb_id,
                        key,
                        same,
                        conf,
                        &model,
                    )
                    .await?;
                    apply_verdict(
                        state,
                        kb_id,
                        item,
                        same,
                        conf,
                        "adjudicated",
                        v.why.as_deref(),
                    )
                    .await?;
                }
                None => {
                    utopia_store::resolution::escalate_review(
                        &state.pool,
                        item.id,
                        "escalate_no_verdict",
                    )
                    .await?;
                }
            }
        }
        // 本轮裁决落库完毕，推给前端刷新审核队列
        state.emit_review(kb_id);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn apply_verdict(
    state: &AppState,
    kb_id: Uuid,
    item: &ReviewItem,
    same: Option<bool>,
    conf: f32,
    via: &str,
    why: Option<&str>,
) -> anyhow::Result<()> {
    // 抽给人的那一份：机器有把握也不动手（0026）。理由写进队列那一列，
    // 界面会说"这一对是抽样给你的"，而不是让人以为裁决器没把握
    if same.is_some() && conf >= AUTO_CONF && sampled_for_a_person(item) {
        let verdict = if same == Some(true) {
            "same"
        } else {
            "different"
        };
        utopia_store::resolution::escalate_review(
            &state.pool,
            item.id,
            &format!("escalate_sample|{verdict} {conf:.2}"),
        )
        .await?;
        return Ok(());
    }
    match same {
        Some(true) if conf >= AUTO_CONF => {
            // 执行闸门（0027）：合并会立刻送出图外的东西——违规、派生、答案——留给人，
            // 把握再高也不动手。人看到的是留下的原因，不是「裁决器没把握」
            let impact = utopia_store::execution_gate::impact_of(
                &state.pool,
                kb_id,
                item.left.id,
                item.right.id,
            )
            .await?;
            if let Some(hold) = utopia_store::execution_gate::hold(&impact) {
                utopia_store::resolution::escalate_review(
                    &state.pool,
                    item.id,
                    &format!("escalate_impact|{hold}"),
                )
                .await?;
                return Ok(());
            }
            let (target, source) =
                utopia_store::resolution::merge_direction(&state.pool, item.left.id, item.right.id)
                    .await?;
            let reason = format!("auto_merged|{via} {conf:.2}");
            match utopia_store::resolution::merge_entities(
                &state.pool,
                kb_id,
                source,
                target,
                None,
                &reason,
            )
            .await
            {
                Ok(_) => {
                    utopia_store::resolution::close_review_auto(
                        &state.pool,
                        item.id,
                        "merged",
                        &reason,
                    )
                    .await?;
                    // 决策台账：AI 自动合并（actor 为空 = 系统）
                    let _ = utopia_store::audit::record_opt(
                        &state.pool,
                        Some(kb_id),
                        None,
                        "review.merge",
                        "review",
                        Some(item.id),
                        serde_json::json!({
                            "left": item.left.name, "right": item.right.name,
                            "score": item.score, "confidence": conf, "via": via,
                            // 模型的一句理由也留下：机器的行不是先例（0025 决定 1），
                            // 但人看 Decisions 时该看得见它凭什么
                            "why": why,
                        }),
                    )
                    .await;
                }
                // 同批次连锁合并可能已吞掉其中一方：转人工而不是让任务失败
                Err(AppError::Conflict(_)) | Err(AppError::NotFound) => {
                    utopia_store::resolution::escalate_review(
                        &state.pool,
                        item.id,
                        "escalate_entity_changed",
                    )
                    .await?;
                }
                Err(e) => return Err(e.into()),
            }
        }
        Some(false) if conf >= AUTO_CONF => {
            utopia_store::resolution::close_review_auto(
                &state.pool,
                item.id,
                "kept",
                &format!("kept_apart|{via} {conf:.2}"),
            )
            .await?;
            let _ = utopia_store::audit::record_opt(
                &state.pool,
                Some(kb_id),
                None,
                "review.keep",
                "review",
                Some(item.id),
                serde_json::json!({
                    "left": item.left.name, "right": item.right.name,
                    "score": item.score, "confidence": conf, "via": via,
                    "why": why,
                }),
            )
            .await;
        }
        _ => {
            utopia_store::resolution::escalate_review(
                &state.pool,
                item.id,
                &format!("escalate_unsure|{via} {conf:.2}"),
            )
            .await?;
        }
    }
    Ok(())
}
