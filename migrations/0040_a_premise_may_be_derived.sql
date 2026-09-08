-- 0030：一条派生事实的前提可以是另一条派生事实。
--
-- 0013 建这张表时写着「前提一律是断言」，理由是「同一次调用的输出会变成下一次
-- 的输入，重跑结果依赖上一轮的残留」。那个理由现在还成立，被守住的方式也没变：
-- **规则之间的反馈发生在同一次 `materialize()` 里，在内存的不动点上**，输入侧
-- 一行 `derived_facts` 都不读。一次推理仍然是 (facts, rules, axioms) 的纯函数。
-- 变的只是：证明从一条链变成一棵树，所以这张表得存得下树的中间那一层。
--
-- 两列二选一而不是加一个 kind 列：外键在两边都留着，删哪一侧都跟得上；
-- `seq` 仍然是**跨两种前提的一个序**——证明读起来是一个顺序，不是两段。
ALTER TABLE fact_derivations ALTER COLUMN premise_fact_id DROP NOT NULL;

ALTER TABLE fact_derivations
    ADD COLUMN premise_derived_id UUID REFERENCES derived_facts(id) ON DELETE CASCADE;

ALTER TABLE fact_derivations
    ADD CONSTRAINT fact_derivations_one_premise
    CHECK ((premise_fact_id IS NOT NULL) <> (premise_derived_id IS NOT NULL));

-- 「这条派生被作废了，哪些派生站在它上面」——与 `fact_derivations_premise_idx`
-- 同一件事，只是另一侧
CREATE INDEX fact_derivations_derived_premise_idx
    ON fact_derivations (premise_derived_id);

-- 一条派生的直接前提，两种前提长成同一副样子。
--
-- **有这个视图是因为读的地方有六处**：实体面板的证明、派生那一档、规则命中
-- 列表、导出、图上的边。每一处都写一遍 `JOIN facts ... UNION ALL JOIN
-- derived_facts ...`，六份里迟早有一份漏掉后半段——而漏掉的表现是「前提少了
-- 一条」，不是报错。`derived` 那一列答的是「这一步还要不要再往下问一次」。
CREATE VIEW derivation_premises AS
SELECT fd.derived_fact_id, fd.seq, FALSE AS derived,
       f.id, f.subject_id, f.predicate_id, f.object_id, f.object_value,
       f.valid_from, f.valid_to, f.confidence, f.invalidated_at
  FROM fact_derivations fd
  JOIN facts f ON f.id = fd.premise_fact_id
UNION ALL
SELECT fd.derived_fact_id, fd.seq, TRUE AS derived,
       d.id, d.subject_id, d.predicate_id, d.object_id, d.object_value,
       d.valid_from, d.valid_to, d.confidence, d.invalidated_at
  FROM fact_derivations fd
  JOIN derived_facts d ON d.id = fd.premise_derived_id;
