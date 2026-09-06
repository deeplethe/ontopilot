-- 0025：治理自己跑，先读台账再裁。
--
-- 每库一个开关 governance（缺省关）。开着：抽取结束、有人裁决之后、打开开关那一刻、
-- 每小时一次，排一个 govern 任务，把等人的重复对按先进先出过一遍；两簇之间看一眼
-- 开关，关掉就停在那里。缺省关：合并是改图的动作，agent 的命中还没在哪个库上量过，
-- 与 auto_type_resolution 缺省开的理由正相反（那一档量过 39/41，且只往子树走一格）。
--
-- agent 的每一笔是 agent_decisions 的一行：建议（proposed，等人接受或改判）、自己
-- 裁了（applied，可撤）、人的回答（accepted / overridden / reverted）、被后来的人裁决
-- 盖过（superseded：同名或同实体的另一对由人定了，这条建议过时，那一对回到队列）。
-- 这张表就是审核台的 Agent 队列，也是接受率 / 改判率 / 撤回数的来源。
ALTER TABLE knowledge_bases ADD COLUMN governance BOOLEAN NOT NULL DEFAULT FALSE;

CREATE TABLE agent_decisions (
    id           UUID PRIMARY KEY,
    kb_id        UUID NOT NULL REFERENCES knowledge_bases(id) ON DELETE CASCADE,
    -- 哪一轮 govern 任务：同一轮里的行是一起看的
    run_id       UUID NOT NULL,
    -- 现在只有 review（重复对）；冲突与事实队列接进来时扩这个 CHECK
    target_kind  TEXT NOT NULL CHECK (target_kind IN ('review')),
    target_id    UUID NOT NULL,
    -- unsure 也留一行：看过了、说不准，人知道它问过
    action       TEXT NOT NULL CHECK (action IN ('merge', 'keep', 'unsure')),
    confidence   REAL NOT NULL DEFAULT 0,
    -- 模型给的一句理由，原话
    reason       TEXT,
    -- 它引用的先例：[{event_id, action, left, right, at}]
    precedents   JSONB NOT NULL DEFAULT '[]',
    status       TEXT NOT NULL CHECK (status IN
                 ('proposed', 'applied', 'accepted', 'overridden', 'reverted', 'superseded')),
    -- applied 且是合并时：entity_merges 的那一行，撤回走它
    merge_id     UUID REFERENCES entity_merges(id) ON DELETE SET NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    decided_at   TIMESTAMPTZ,
    decided_by   UUID REFERENCES users(id)
);
CREATE INDEX agent_decisions_kb_idx ON agent_decisions (kb_id, created_at DESC);
-- 一个目标同时只有一条开着的建议
CREATE UNIQUE INDEX agent_decisions_open_idx
    ON agent_decisions (target_kind, target_id) WHERE status = 'proposed';
