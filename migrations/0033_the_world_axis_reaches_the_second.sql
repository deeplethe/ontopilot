-- 0024：世界轴到秒为止。
--
-- 精度梯子从 year / month / day 延到 hour / minute / second；结束端照旧多一个 unknown。
-- 记忆日志的一行打到分、工单到秒、备忘录说「下午三点」——账本此前一律记成那一天，
-- 源头知道的比账本多。不再往下：没有哪个源头陈述到亚秒；记录轴留微秒是因为那是
-- 我们自己的钟。
--
-- 存的值截到自己的精度：date_trunc(precision, value, 'UTC') = value。精度的名字就是
-- date_trunc 的字段名，一条表达式说完；三参形式按 UTC 截，不随会话时区变（两参形式
-- 跟着 TimeZone 设置走，进 CHECK 就不确定了）。有了它，存的、显示的、导出的永远是
-- 同一句话。
--
-- 三值逻辑的坑（0003 那条注释）：`precision IN (…)` 在 precision 为 NULL 时是 NULL，
-- 与 TRUE 相与仍是 NULL，而 CHECK 遇 NULL 判通过。所以每个分支都先写 IS NOT NULL。

-- facts：先把不合的行归一。表示法的修正，不是含义的——精度早就宣布了那些位是噪音
UPDATE facts SET valid_from = date_trunc(valid_from_precision, valid_from, 'UTC')
 WHERE valid_from IS NOT NULL AND valid_from_precision IN ('year', 'month', 'day')
   AND valid_from <> date_trunc(valid_from_precision, valid_from, 'UTC');
UPDATE facts SET valid_to = date_trunc(valid_to_precision, valid_to, 'UTC')
 WHERE valid_to IS NOT NULL AND valid_to_precision IN ('year', 'month', 'day')
   AND valid_to <> date_trunc(valid_to_precision, valid_to, 'UTC');

ALTER TABLE facts
    DROP CONSTRAINT facts_from_precision_matches_date,
    DROP CONSTRAINT facts_to_precision_matches_date,
    ADD CONSTRAINT facts_from_precision_matches_date CHECK (
        (valid_from IS NULL AND valid_from_precision IS NULL)
        OR (valid_from IS NOT NULL
            AND valid_from_precision IS NOT NULL
            AND valid_from_precision IN ('year', 'month', 'day', 'hour', 'minute', 'second')
            AND valid_from = date_trunc(valid_from_precision, valid_from, 'UTC'))),
    ADD CONSTRAINT facts_to_precision_matches_date CHECK (
        (valid_to IS NOT NULL
            AND valid_to_precision IS NOT NULL
            AND valid_to_precision IN ('year', 'month', 'day', 'hour', 'minute', 'second')
            AND valid_to = date_trunc(valid_to_precision, valid_to, 'UTC'))
        OR (valid_to IS NULL AND (valid_to_precision IS NULL OR valid_to_precision = 'unknown')));

-- pending_facts（0018）：同一张表的同一条不变量。从前没有约束，先归一再加
UPDATE pending_facts SET valid_from = date_trunc(valid_from_precision, valid_from, 'UTC')
 WHERE valid_from IS NOT NULL AND valid_from_precision IN ('year', 'month', 'day')
   AND valid_from <> date_trunc(valid_from_precision, valid_from, 'UTC');
UPDATE pending_facts SET valid_to = date_trunc(valid_to_precision, valid_to, 'UTC')
 WHERE valid_to IS NOT NULL AND valid_to_precision IN ('year', 'month', 'day')
   AND valid_to <> date_trunc(valid_to_precision, valid_to, 'UTC');
ALTER TABLE pending_facts
    ADD CONSTRAINT pending_from_precision_matches_date CHECK (
        (valid_from IS NULL AND valid_from_precision IS NULL)
        OR (valid_from IS NOT NULL
            AND valid_from_precision IS NOT NULL
            AND valid_from_precision IN ('year', 'month', 'day', 'hour', 'minute', 'second')
            AND valid_from = date_trunc(valid_from_precision, valid_from, 'UTC'))),
    ADD CONSTRAINT pending_to_precision_matches_date CHECK (
        (valid_to IS NOT NULL
            AND valid_to_precision IS NOT NULL
            AND valid_to_precision IN ('year', 'month', 'day', 'hour', 'minute', 'second')
            AND valid_to = date_trunc(valid_to_precision, valid_to, 'UTC'))
        OR (valid_to IS NULL AND (valid_to_precision IS NULL OR valid_to_precision = 'unknown')));

-- derived_facts：派生行的一端若带精度，精度跟着**赢下这一端的那条前提**走（0024 改掉了
-- 「取所有前提里最粗的」——year 标在一个 6 月 1 日的值上，值与标签互相矛盾）。先按前提
-- 修正现有行的精度；修不回来的（前提已作废）置空——算出来的界，没有原文的精度可言
UPDATE derived_facts d SET valid_from_precision = (
    SELECT pf.valid_from_precision
      FROM fact_derivations fd JOIN facts pf ON pf.id = fd.premise_fact_id
     WHERE fd.derived_fact_id = d.id AND pf.valid_from = d.valid_from
       AND pf.valid_from_precision IS NOT NULL
     ORDER BY array_position(ARRAY['year', 'month', 'day', 'hour', 'minute', 'second'],
                             pf.valid_from_precision)
     LIMIT 1)
 WHERE d.valid_from IS NOT NULL AND d.valid_from_precision IS NOT NULL
   AND d.valid_from <> date_trunc(d.valid_from_precision, d.valid_from, 'UTC');
UPDATE derived_facts d SET valid_to_precision = (
    SELECT pf.valid_to_precision
      FROM fact_derivations fd JOIN facts pf ON pf.id = fd.premise_fact_id
     WHERE fd.derived_fact_id = d.id AND pf.valid_to = d.valid_to
       AND pf.valid_to_precision IS NOT NULL AND pf.valid_to_precision <> 'unknown'
     ORDER BY array_position(ARRAY['year', 'month', 'day', 'hour', 'minute', 'second'],
                             pf.valid_to_precision)
     LIMIT 1)
 WHERE d.valid_to IS NOT NULL AND d.valid_to_precision IS NOT NULL
   AND d.valid_to <> date_trunc(d.valid_to_precision, d.valid_to, 'UTC');
UPDATE derived_facts SET valid_from_precision = NULL
 WHERE valid_from IS NOT NULL AND valid_from_precision IS NOT NULL
   AND (valid_from_precision NOT IN ('year', 'month', 'day', 'hour', 'minute', 'second')
        OR valid_from <> date_trunc(valid_from_precision, valid_from, 'UTC'));
UPDATE derived_facts SET valid_to_precision = NULL
 WHERE valid_to IS NOT NULL AND valid_to_precision IS NOT NULL
   AND (valid_to_precision NOT IN ('year', 'month', 'day', 'hour', 'minute', 'second')
        OR valid_to <> date_trunc(valid_to_precision, valid_to, 'UTC'));

ALTER TABLE derived_facts
    DROP CONSTRAINT derived_from_precision_needs_date,
    DROP CONSTRAINT derived_to_precision_needs_date,
    ADD CONSTRAINT derived_from_precision_matches_date CHECK (
        valid_from_precision IS NULL
        OR (valid_from IS NOT NULL
            AND valid_from_precision IN ('year', 'month', 'day', 'hour', 'minute', 'second')
            AND valid_from = date_trunc(valid_from_precision, valid_from, 'UTC'))),
    ADD CONSTRAINT derived_to_precision_matches_date CHECK (
        valid_to_precision IS NULL
        OR (valid_to IS NOT NULL
            AND valid_to_precision IN ('year', 'month', 'day', 'hour', 'minute', 'second')
            AND valid_to = date_trunc(valid_to_precision, valid_to, 'UTC')));
