-- 名字是关于实体的一条事实（0041 决定 1）。
--
-- 从前一个实体的名字分两处放：`entities.canonical_name` 一个，`entities.aliases` 一串。
-- 别名只有合并会写，没有出处、没有时间、撤不回单个，抽取读到的「简称海探1」进不来。
-- 现在每个名字都是 `known_as` 这条内建属性上的一条值事实：`object_value = {"value": 名字}`，
-- `object_id` 为空——**不成节点**，画布只画带 object_id 的边，和数量值走同一个通道（#586）。
-- 时态引擎只对声明了 functional / inverse_functional 的状态关系收口、记冲突，`known_as`
-- 两者都不是（重名本来就有），第二个名字不会关掉第一个。
--
-- `canonical_name` 留着，是界面上显示的那一个，同时它也是一条名字事实。
--
-- **已经合并掉的实体要小心。** 它的名字现在躺在存活者的 `aliases` 里，合并日志
-- `entity_merges` 靠 `moved_subject_facts` 记「搬过哪些事实」、撤回时按它搬回去。
-- 名字变成事实之后，新的合并会把名字事实当普通事实搬、撤回时搬回来，不用任何特判。
-- 迁移时已经在合并状态的那些，要补成「当初就是这么搬的」：
--   它的名字事实挂到合并链末端的存活者身上，并把这条事实的 id 追加进链上每一条
--   未撤回合并的 `moved_subject_facts`——撤回链上任何一环，名字都跟着那一环回去；
--   存活者已经有同一个名字（重名合并最常见：「Apple」并进「Apple」），就把它作废，
--   并同样追加进链上每一条的 `invalidated_facts`——合并本来就会这样去重，撤回时一并复活。

-- 1. 每个库一条内建属性。按需也会建（`names::ensure_known_as`），这里给存量库补上
INSERT INTO relation_types (id, kb_id, key, label, kind, datatype, temporal, builtin, description)
SELECT gen_random_uuid(), k.id, 'known_as', 'known as', 'attribute', 'text', 'state', TRUE,
       'A name a text uses for this entity. Each name is a fact with its source and both clocks; a name is a value and never becomes a node.'
  FROM knowledge_bases k
ON CONFLICT (kb_id, key) DO NOTHING;

-- 2. 存活实体的本名
INSERT INTO facts (id, kb_id, subject_id, predicate_id, object_value, recorded_at, attested_from, confidence)
SELECT gen_random_uuid(), e.kb_id, e.id, r.id, jsonb_build_object('value', e.canonical_name),
       e.created_at, e.created_at, 1.0
  FROM entities e
  JOIN relation_types r ON r.kb_id = e.kb_id AND r.key = 'known_as'
 WHERE e.merged_into IS NULL;

-- 3. 已合并实体：沿 merged_into 走到存活者，记下经过的每个节点
CREATE TEMP TABLE name_walk ON COMMIT DROP AS
WITH RECURSIVE walk(origin, node, depth) AS (
    SELECT e.id, e.id, 0 FROM entities e WHERE e.merged_into IS NOT NULL
    UNION ALL
    SELECT w.origin, e.merged_into, w.depth + 1
      FROM walk w JOIN entities e ON e.id = w.node
     WHERE e.merged_into IS NOT NULL AND w.depth < 64
)
SELECT origin, node, depth FROM walk;

CREATE TEMP TABLE merged_names ON COMMIT DROP AS
SELECT gen_random_uuid() AS fact_id, x.id AS origin, x.kb_id, x.canonical_name, x.created_at,
       (SELECT w.node FROM name_walk w WHERE w.origin = x.id ORDER BY w.depth DESC LIMIT 1) AS survivor
  FROM entities x
 WHERE x.merged_into IS NOT NULL;

INSERT INTO facts (id, kb_id, subject_id, predicate_id, object_value, recorded_at, attested_from, confidence)
SELECT m.fact_id, m.kb_id, m.survivor, r.id, jsonb_build_object('value', m.canonical_name),
       m.created_at, m.created_at, 1.0
  FROM merged_names m
  JOIN relation_types r ON r.kb_id = m.kb_id AND r.key = 'known_as';

UPDATE entity_merges em
   SET moved_subject_facts = em.moved_subject_facts || ARRAY(
           SELECT m.fact_id FROM merged_names m
             JOIN name_walk w ON w.origin = m.origin
            WHERE w.node = em.source_id)
 WHERE em.reverted_at IS NULL
   AND EXISTS (SELECT 1 FROM merged_names m JOIN name_walk w ON w.origin = m.origin
                WHERE w.node = em.source_id);

-- 同一个存活者身上重复的名字：存活者自己的本名留着，合并来的里面留最早记下的，其余作废
CREATE TEMP TABLE merged_name_dups ON COMMIT DROP AS
SELECT m.fact_id, m.origin
  FROM merged_names m
 WHERE EXISTS (SELECT 1 FROM entities s
                WHERE s.id = m.survivor AND lower(s.canonical_name) = lower(m.canonical_name))
    OR EXISTS (SELECT 1 FROM merged_names o
                WHERE o.survivor = m.survivor AND lower(o.canonical_name) = lower(m.canonical_name)
                  AND (o.created_at, o.fact_id) < (m.created_at, m.fact_id));

UPDATE facts SET invalidated_at = now()
 WHERE id IN (SELECT fact_id FROM merged_name_dups);

UPDATE entity_merges em
   SET invalidated_facts = em.invalidated_facts || ARRAY(
           SELECT d.fact_id FROM merged_name_dups d
             JOIN name_walk w ON w.origin = d.origin
            WHERE w.node = em.source_id)
 WHERE em.reverted_at IS NULL
   AND EXISTS (SELECT 1 FROM merged_name_dups d JOIN name_walk w ON w.origin = d.origin
                WHERE w.node = em.source_id);

-- 4. 存活者别名里剩下的（不对应任何已合并实体本名的，理论上不该有，有就照记）
INSERT INTO facts (id, kb_id, subject_id, predicate_id, object_value, recorded_at, attested_from, confidence)
SELECT gen_random_uuid(), e.kb_id, e.id, r.id, jsonb_build_object('value', a.alias),
       e.updated_at, e.updated_at, 1.0
  FROM entities e
  JOIN relation_types r ON r.kb_id = e.kb_id AND r.key = 'known_as'
  CROSS JOIN LATERAL (SELECT DISTINCT ON (lower(x)) x AS alias FROM unnest(e.aliases) x) a
 WHERE e.merged_into IS NULL
   AND NOT EXISTS (SELECT 1 FROM facts f
                    WHERE f.subject_id = e.id AND f.predicate_id = r.id
                      AND lower(f.object_value->>'value') = lower(a.alias));

ALTER TABLE entities DROP COLUMN aliases;

-- 召回按名字等值查：值事实的小写值上建索引（数量值也在里面，不妨碍）
CREATE INDEX facts_value_text_idx ON facts (kb_id, lower(object_value->>'value'))
 WHERE invalidated_at IS NULL AND object_value IS NOT NULL;
