-- 0025 第四刀：保险丝。
--
-- 七天内人撤回 agent 的自动合并两次，开关自动关掉并发一条告警：它替人做主做错了
-- 两回，该停下来等人再开。只数这次打开之后的撤回——人重新打开时，之前那两次不再
-- 算，否则一开就再跳。governance_since 记的就是这次打开的时刻。
ALTER TABLE knowledge_bases ADD COLUMN governance_since TIMESTAMPTZ;
