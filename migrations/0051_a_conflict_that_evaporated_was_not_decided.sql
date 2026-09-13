-- 蒸发掉的冲突不是被裁决的（#612 / #613）。
--
-- 从前 `temporal::list_conflicts` 是一条读，可它在 SELECT 之前先跑一条 UPDATE：
-- 把「两边有一边已经作废」的开着的冲突改成
-- `status='resolved', resolution='stale', resolved_at=now()`。清理本身是对的——
-- 一条边没了，围着它的冲突就无题可裁，留在队列里只会让人对着僵尸做决定。错的是
-- 它在哪儿做、以及做完之后那行说了什么：
--
--   `resolved_at` 记的是**有人打开了审核页**的时刻，不是冲突失效的时刻。谁把它
--   当裁决时间读——导出、审计、「冲突平均多久裁完」——读到的是一次页面访问。
--   `record_axis` 的 `status='open' OR resolved_at > T` 正是这么读的，于是
--   「T 时刻这条冲突开着吗」的答案取决于此后谁开过那一页。
--
--   `stale` 不在 `resolution` 注释列的三个值里，而那一列没有 CHECK，文档写的集合
--   和真实的集合早就分叉了（#613）。它跟另外三个也不是一类东西：closed /
--   kept_both / rejected_new 是人做的决定，stale 是「这道题没了」。
--
--   没人打开审核页的库，陈冲突就一直开着——状态取决于谁看过。
--
-- 这一刀三件事：
--
--   1. 蒸发给自己一个状态 `withdrawn`。`resolution` 从此只装人的三个决定，并且
--      终于有了 CHECK。读的一方不用再分辨「这个 resolution 是决定还是清理」——
--      status 说了。
--
--   2. 退场发生在**作废的那一刻**，由 facts 上的触发器做，不由读路径做。
--      置 `invalidated_at` 的地方有十二处，撤销作废还有四处；挨个改一遍迟早漏一个，
--      而且下一个新写法还会漏。不变式是「一条边作废了，围着它的冲突就无题可裁」,
--      它该钉在数据变的地方。撤销作废也照此反向：两边都活过来，冲突重新开着。
--
--   3. 现有数据能修就修：stale 行的 `resolved_at` 换成两条事实里**先**作废的那个
--      时刻——那才是这道题消失的时刻，而它就在库里。不是抹成 NULL：抹掉会让
--      `record_axis` 把这些冲突当成「从来没开过」，那是另一种假话。

ALTER TABLE fact_conflicts DROP CONSTRAINT fact_conflicts_status_check;
ALTER TABLE fact_conflicts ADD CONSTRAINT fact_conflicts_status_check
    CHECK (status IN ('open', 'resolved', 'withdrawn'));

-- 先修数据，再上 resolution 的 CHECK，否则现有的 stale 行会把迁移顶回去
UPDATE fact_conflicts c
   SET status = 'withdrawn', resolution = NULL,
       resolved_at = (SELECT min(f.invalidated_at) FROM facts f
                       WHERE f.id IN (c.old_fact_id, c.new_fact_id)
                         AND f.invalidated_at IS NOT NULL)
 WHERE c.resolution = 'stale';

-- 该退而没退的（这个库的审核页没人打开过）一并退掉，时刻同样取先作废的那一个
UPDATE fact_conflicts c
   SET status = 'withdrawn', resolution = NULL,
       resolved_at = (SELECT min(f.invalidated_at) FROM facts f
                       WHERE f.id IN (c.old_fact_id, c.new_fact_id)
                         AND f.invalidated_at IS NOT NULL)
 WHERE c.status = 'open'
   AND EXISTS (SELECT 1 FROM facts f
                WHERE f.id IN (c.old_fact_id, c.new_fact_id)
                  AND f.invalidated_at IS NOT NULL);

ALTER TABLE fact_conflicts ADD CONSTRAINT fact_conflicts_resolution_check
    CHECK (resolution IS NULL OR resolution IN ('closed', 'kept_both', 'rejected_new'));

-- 触发器：作废与撤销作废各一支。
--
-- `AFTER UPDATE`：反向那一支要查「另一边现在还作废着吗」，得看见更新之后的 facts。
-- `OF invalidated_at` 加 `WHEN`：事实上的其它更新（改置信度、补结束日期）多得多，
-- 这个函数不该为它们醒来。
CREATE FUNCTION fact_conflicts_follow_invalidation() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.invalidated_at IS NOT NULL THEN
        UPDATE fact_conflicts
           SET status = 'withdrawn', resolution = NULL, resolved_at = NEW.invalidated_at
         WHERE status = 'open' AND NEW.id IN (old_fact_id, new_fact_id);
    ELSE
        -- 撤销作废：只有**两边都活着**才重新开着。一条边回来了而另一条还作废着，
        -- 这道题仍然无解
        UPDATE fact_conflicts c
           SET status = 'open', resolved_at = NULL
         WHERE c.status = 'withdrawn' AND NEW.id IN (c.old_fact_id, c.new_fact_id)
           AND NOT EXISTS (SELECT 1 FROM facts f
                            WHERE f.id IN (c.old_fact_id, c.new_fact_id)
                              AND f.invalidated_at IS NOT NULL);
    END IF;
    RETURN NULL;
END;
$$;

CREATE TRIGGER facts_invalidation_follows_through_to_conflicts
    AFTER UPDATE OF invalidated_at ON facts
    FOR EACH ROW
    WHEN (OLD.invalidated_at IS DISTINCT FROM NEW.invalidated_at)
    EXECUTE FUNCTION fact_conflicts_follow_invalidation();

COMMENT ON COLUMN fact_conflicts.status IS
    'open = 等人裁；resolved = 人裁了，resolution 说怎么裁的；withdrawn = 有一边作废了，这道题没了（0051）';
COMMENT ON COLUMN fact_conflicts.resolution IS
    '只装人的决定：closed | kept_both | rejected_new。withdrawn 的行这一列是 NULL（0051）';
COMMENT ON COLUMN fact_conflicts.resolved_at IS
    '这条冲突离开队列的时刻：resolved 时是裁决时刻，withdrawn 时是先作废的那一边的作废时刻（0051）';
