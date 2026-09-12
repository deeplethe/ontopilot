-- 环违规按环定键，不按遍历的起点定键（#618）。
--
-- `utopia_reason::cycles` 从 `adj` 的每个节点各走一遍，`left` 取这一次遍历的第一条
-- 边、`right` 取最后一条；而 `adj` 是 HashMap，起点顺序每次不同。量过：一个三元环、
-- 同一份输入，`check()` 跑 200 次给出三种 `(left, right, path)`——三个旋转都来过。
--
-- `axiom_violations` 按 `(kb_id, kind, left_fact, right_fact)` 唯一，所以换个旋转
-- 就是换一行。真正的后果不是多一行，是**人的裁决会悄悄失效**：重算会删掉这一轮没
-- 算出来的 open 行、留下裁过的行，于是同一个环以新键插成 open，而「重开」那一支
-- 按键匹配、匹配不上就不触发。人裁过的环第二天原样回来，裁决记录躺在旁边，指着
-- 一个再也不会被算出来的键。
--
-- 代码那边已经改成报之前转到最小事实起头（同一刀）。这里修既有的行。

-- open 的不用修，删掉就行：每一轮推理本来就会清掉这一轮没算出来的 open 行、
-- 把算出来的重新插上，下一轮它们会以规范形状回来。
DELETE FROM axiom_violations WHERE kind = 'cycle' AND status = 'open';

-- 裁过的不能删——那是人的决定。按行上存着的 path 转到规范形状，再改键。
CREATE TEMP TABLE cycle_canon ON COMMIT DROP AS
SELECT v.id, v.kb_id, v.detected_at,
       v.path[m.k:array_length(v.path, 1)] || v.path[1:m.k - 1] AS rotated
  FROM axiom_violations v
 CROSS JOIN LATERAL (
       SELECT i AS k FROM generate_subscripts(v.path, 1) AS i
        ORDER BY v.path[i] LIMIT 1
 ) m
 WHERE v.kind = 'cycle' AND v.status <> 'open' AND array_length(v.path, 1) > 0;

-- 转完可能两行撞在一起：同一个环被两种旋转各裁过一次。留最早发现的那一行。
DELETE FROM axiom_violations WHERE id IN (
    SELECT id FROM (
        SELECT id,
               row_number() OVER (
                   PARTITION BY kb_id, rotated[1], rotated[array_length(rotated, 1)]
                   ORDER BY detected_at, id) AS rn
          FROM cycle_canon) t
     WHERE t.rn > 1);

UPDATE axiom_violations v
   SET path       = c.rotated,
       left_fact  = c.rotated[1],
       right_fact = c.rotated[array_length(c.rotated, 1)]
  FROM cycle_canon c
 WHERE v.id = c.id;
