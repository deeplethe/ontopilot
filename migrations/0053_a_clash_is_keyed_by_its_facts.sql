-- 互斥违规按组里的事实定键，并且只有同时成立才算（#624、#634）。
--
-- asymmetry、functional 从前以扫描时遇到的头两条做 `(left_fact, right_fact)`：同一处
-- 冲突换个取数顺序就是另一个键，而 `axiom_violations` 按 `(kb_id, kind, left_fact,
-- right_fact)` 唯一——人裁过的那一行被绕开，与 #618 的环同一个病（0052）。代码那边
-- 改成：一处违规是一组同时成立、彼此冲突的事实，`path` 是整组按 id 排序，left/right
-- 是它的首尾。
--
-- 同一刀里还有两件：检查开始看有效期（前后相接的两个值是接任），inverse_functional
-- 有了自己的种类（从前与 functional 共用一个）。这里修既有的行。

ALTER TABLE axiom_violations
    DROP CONSTRAINT axiom_violations_kind_check,
    ADD CONSTRAINT axiom_violations_kind_check CHECK (kind IN (
        'self_loop', 'asymmetry', 'cycle', 'functional', 'inverse_functional',
        'signature', 'derived_contradiction'
    ));

-- open 的删掉：下一轮推理以新形状算回来。其中那些接任被报成的矛盾不会再回来。
DELETE FROM axiom_violations
 WHERE kind IN ('asymmetry', 'functional') AND status = 'open';

-- 裁过的不能删——那是人的决定。先把记成 functional 的宾语侧违规改回它的种类：
-- 两条事实宾语相同、主语不同，那是 inverse_functional 算出来的。
UPDATE axiom_violations v
   SET kind = 'inverse_functional'
  FROM facts l, facts r
 WHERE v.kind = 'functional'
   AND l.id = v.left_fact AND r.id = v.right_fact
   AND l.object_id = r.object_id AND l.subject_id <> r.subject_id;

-- 再规范成新形状。从前记的都是一对，所以组就是这两条。
CREATE TEMP TABLE clash_canon ON COMMIT DROP AS
SELECT id, kb_id, kind, detected_at,
       LEAST(left_fact, right_fact) AS lo,
       GREATEST(left_fact, right_fact) AS hi
  FROM axiom_violations
 WHERE kind IN ('asymmetry', 'functional', 'inverse_functional') AND status <> 'open';

-- 规范完可能两行撞在一起：同一对被两种顺序各裁过一次。留最早发现的那一行。
DELETE FROM axiom_violations WHERE id IN (
    SELECT id FROM (
        SELECT id,
               row_number() OVER (
                   PARTITION BY kb_id, kind, lo, hi
                   ORDER BY detected_at, id) AS rn
          FROM clash_canon) t
     WHERE t.rn > 1);

UPDATE axiom_violations v
   SET left_fact  = c.lo,
       right_fact = c.hi,
       path       = ARRAY[c.lo, c.hi]
  FROM clash_canon c
 WHERE v.id = c.id;
