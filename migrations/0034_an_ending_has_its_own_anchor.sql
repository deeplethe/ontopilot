-- #393（0022 的第一个开问题）：结束端要有自己的锚点。
--
-- 0022 给每行一个锚点 attested_at——各次观察里最早那份文档的日期：没起点的事实从它起
-- 成立，结束了不知哪天的到它为止。一个锚点装不下两头：没起点的裸行被一句「不再担任，
-- 日期未记录」关上时，起点要用第一份证据的日期、终点要用说出结束的那份文档的日期。
-- 于是那种行从前关不上——去重把结束并进裸行，「它结束了」这唯一带来的信息就丢了。
--
-- attested_at 改名 attested_from，只管起点；新加 attested_to，只管「结束了不知哪天」的
-- 终点——有这个状态才有它，没有就是 NULL，CHECK 钉住等价。三值逻辑的坑照旧：先写
-- IS NOT NULL 再比字面量，否则精度为 NULL 的行会让 CHECK 静默放行。
ALTER TABLE facts RENAME COLUMN attested_at TO attested_from;
ALTER TABLE facts ADD COLUMN attested_to TIMESTAMPTZ;

-- 回填：结束未知的行，说出结束的就是它自己的证据（0022 第一刀关行时把锚点换成了它，
-- 新立的结束行锚点本来就是它那份文档）
UPDATE facts SET attested_to = attested_from
 WHERE valid_to IS NULL AND valid_to_precision IS NOT NULL AND valid_to_precision = 'unknown';

ALTER TABLE facts ADD CONSTRAINT facts_ended_unknown_has_anchor CHECK (
    (valid_to IS NULL AND valid_to_precision IS NOT NULL AND valid_to_precision = 'unknown')
    = (attested_to IS NOT NULL));
