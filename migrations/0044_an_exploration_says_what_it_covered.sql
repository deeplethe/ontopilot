-- 一次映射探索扫了什么、丢了什么、剩下什么（#503）。
--
-- 从前一轮探索只留下两样东西：`concept_mappings` 里若干行，以及一条
-- 「一条都没提出来」的告警。**十一条提议对着一张八十列的宽表，与十一条
-- 对着一个刚好覆盖完的小库，页面上长得一模一样**——覆盖率没有任何地方说。
--
-- 而丢弃是静默的。`explore_mappings` 对每条提议有四处 `continue`：源名对不上、
-- kind 不是 metric/dimension、概念类查不到、definition 不是对象。四处都不计数。
-- 实测踩过一次：源叫 `tpch-2026-09-08-12-30`，模型看着 schema 回的 `source`
-- 是 `tpch`，十二条提议一条不剩地被吞掉，任务照样 done。
--
-- 所以把一轮探索本身记下来。**它不是审计事件**：审计答的是「谁做了什么」，
-- 这张表答的是「这一轮看见了多大的东西、覆盖了多少」，是下一次要不要再跑、
-- 要不要给列加注释的依据。

CREATE TABLE mapping_exploration_runs (
    id          UUID PRIMARY KEY,
    kb_id       UUID NOT NULL REFERENCES knowledge_bases(id) ON DELETE CASCADE,
    started_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ,

    -- 这一轮读了哪些源
    sources          TEXT[] NOT NULL DEFAULT '{}',
    tables_scanned   INTEGER NOT NULL DEFAULT 0,
    columns_scanned  INTEGER NOT NULL DEFAULT 0,
    -- schema 文本撞了上限：**提示词里没有的表，模型没有机会提**。
    -- 覆盖率低的时候这一位说明是「没看见」还是「看见了没提」
    schema_truncated BOOLEAN NOT NULL DEFAULT FALSE,

    -- 这一轮允许提几条（按表数放大，不再是写死的 12）
    cap       INTEGER NOT NULL DEFAULT 0,
    -- 模型回了几条 / 落库几条。两者之差就是被丢掉的
    returned  INTEGER NOT NULL DEFAULT 0,
    accepted  INTEGER NOT NULL DEFAULT 0,
    -- 丢弃分类：{"source": n, "kind": n, "type": n, "definition": n}。
    -- 分开数是因为该做的事不同——源名对不上要改源名或改提示词，
    -- 概念类查不到是本体缺了类，definition 不成形是模型没听懂格式
    dropped   JSONB NOT NULL DEFAULT '{}'::jsonb,

    -- 有至少一条提议落在上面的表，`schema.table` 原样。覆盖率的分子；
    -- 分母是 tables_scanned
    tables_covered TEXT[] NOT NULL DEFAULT '{}',

    -- 跑挂了的那一轮也留一行：**失败与「跑了但什么都没提」不是一回事**，
    -- 而从前两者在页面上都是「没有新提议」
    error TEXT
);

-- 页面只要最近那几轮
CREATE INDEX mapping_exploration_runs_idx
    ON mapping_exploration_runs (kb_id, started_at DESC);
