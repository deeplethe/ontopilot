-- 0025 第二刀：攒批判不定的对，agent 逐条带工具再看一遍，看完还定不了就留一个
-- 具体的问题给人，不猜。
--
-- question：defer 留下的那一个问题——人一眼能答的那种，点名哪条事实或哪份文档
--           能定。只有 action = unsure 的行才有。
-- trace：   它看了什么：[{tool, args, note}]，一条一次查询。轨迹就是解释——
--           「查了 A 的事实、读了两段原文、翻了台账里 Apple 的决定」。
-- calls：   这一行在循环里花了几次模型调用；每库每天的循环预算按它算。
ALTER TABLE agent_decisions
    ADD COLUMN question TEXT,
    ADD COLUMN trace    JSONB NOT NULL DEFAULT '[]',
    ADD COLUMN calls    INT NOT NULL DEFAULT 0;
