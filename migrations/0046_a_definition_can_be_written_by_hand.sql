-- 一条口径可以由人从零写（#562）。
--
-- `concept_mappings` 的行从前只有一条来路：探索提议、人确认。一个数据团队手上有
-- 自己的指标口径文档，却没有地方把它填进去——而测量台已经量过，口径进了问数的
-- 提示词，宽表语料从 1/18 到 17/18（#520）。缺的不是结构，是这条入口。
--
-- 记下是谁写的。探索提的 `written_by` 为空，人写的记人；页面据此标「人写」还是
-- 「探索提的」。`decided_by` 分不出这件事：探索提的经人确认之后同样有 decided_by。
ALTER TABLE concept_mappings
    ADD COLUMN written_by UUID REFERENCES users(id);
