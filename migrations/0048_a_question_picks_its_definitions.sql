-- 问数按问题挑口径，而不是把前 30 条全塞进提示词（#574）。
--
-- `chat.rs` 从前读 `confirmed(kb, 30)`：按概念名字典序取前三十条。二十七条口径
-- 的上界跑到 17/18（#520），离顶只差三条；一个真实的库有一百条口径时，字母
-- 靠后的七十条进不了提示词，问数照样看不见——「二十七条效果不错」推不到一百条。
--
-- 所以口径也做成可检索的：向量 + 词面两路，RRF 合并，取前几条。形状照
-- `entity_types` 的嵌入索引（`embedding` / `embedded_model` / `embedded_text`）——
-- 模型换了或文本改了就重嵌，谁都不用记得去刷新。

ALTER TABLE concept_mappings
    ADD COLUMN embedding      vector,
    ADD COLUMN embedded_model TEXT,
    ADD COLUMN embedded_text  TEXT;
