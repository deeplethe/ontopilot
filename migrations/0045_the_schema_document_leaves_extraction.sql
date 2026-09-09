-- schema 文档只做检索语料，不进抽取（0035 决定 7，#553）。
--
-- 挂载数据源时把 schema 摄成一份 markdown，好让问数 `search_chunks` 找得到表。
-- 这份文档从前跟别的文档一样进抽取，而抽取器看见的正是探索刚建的 Metric /
-- Dimension 两个类——于是每个列名都被抽成一个实体。宽表语料上量过：四十个
-- 概念实体里二十八个是列名，`amt_pay` 是一个 Metric，`dw.dim_shop` 是一个
-- Dimension 还挂着八条事实。
--
-- 「只检索、不学习」是这份文档所属来源的性质，记在 `sources.config`——那一列
-- 本来就是 kind 专属配置的 JSONB。没有拿来源的名字当规则（「叫 Data schemas 的
-- 文件夹不抽取」）：那是拿命名约定冒充类型保证，0009 警告过的那一类缺陷。

-- 流水线走到抽取那一步、发现来源不要抽取时，文档得有个状态说这件事。
-- 留在 `none` 会让 Library 把它显示成「还没排到」，而它永远不会排到
ALTER TABLE documents DROP CONSTRAINT IF EXISTS documents_graph_status_check;
ALTER TABLE documents
    ADD CONSTRAINT documents_graph_status_check
    CHECK (graph_status IN ('none', 'queued', 'extracting', 'done', 'failed', 'skipped'));

-- 已有的库里那个文件夹是 `sync_schema_doc` 用固定名字建的，补上标记。
-- 这里按名字找是一次性回填，不是运行时规则
UPDATE sources
   SET config = config || '{"extract": false}'::jsonb
 WHERE kind = 'folder' AND name = 'Data schemas';

-- 已有的库里这份文档可能早就排过抽取。没抽成的（还没排到、或者失败了）标成
-- skipped：留着 `failed`，Library 的「重试失败」会把它原样送回抽取，而这正是
-- 上面说了不做的事。抽完了的（`done`）不动——它抽出来的列名实体要靠重建清掉
UPDATE documents d
   SET graph_status = 'skipped', graph_error = NULL
  FROM sources s
 WHERE s.id = d.source_id
   AND s.config -> 'extract' = 'false'::jsonb
   AND d.graph_status IN ('none', 'failed');
