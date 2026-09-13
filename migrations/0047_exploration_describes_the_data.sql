-- 探索写一段库的数据描述，并列出它拿不准的地方；约定由人写（#570）。
--
-- 量过的事实（#520）：宽表语料上问数没有约定 2/18，约定写成一页散文 14/18。
-- 约定分两半——结构那半（一行是什么、金额单位、状态码含义、时间轴、哪两列长得像）
-- schema 与注释里读得出来，探索能生成；约定那半（测试单不算数、有效订单是 2/3/4、
-- GMV 用实付不用优惠前）schema 里没有，探索生成不了，**而且不能让它猜**：
-- 猜出来的约定进了提示词，问数会照着算，比没有更糟。
--
-- 所以三个字段，两个来路：
--   data_description  探索生成，每次探索重写。只写 schema 说了的
--   data_questions    探索生成，它拿不准、需要人答的那几个问题
--   data_conventions  人写。探索不碰——混在一个字段里，下一次探索就把人的答案盖了
-- 问数与探索的提示词读前两个（描述）与第三个（约定）；问题清单给页面。

ALTER TABLE knowledge_bases
    ADD COLUMN data_description TEXT,
    ADD COLUMN data_questions   JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN data_conventions TEXT;
