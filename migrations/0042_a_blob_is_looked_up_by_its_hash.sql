-- #516：purge 交出原文之前要问「还有谁引用这个 sha」，问的是两张表，两张表
-- 都没有按 sha 查的索引。
--
-- documents 上那个 (kb_id, sha256) 的唯一索引服务不了这一问：原文按内容寻址、
-- 跨库共用，问的时候故意不带 kb_id，前导列对不上就是整表扫。document_versions
-- 只有 (document_id, version)，按 sha 问一样是整表扫。而这两问跑在 purge 的事务
-- 里，此时来源锁和文档行锁都已握着，扫多久别的写入就等多久。
--
-- CI 的 migrations job 会把全部迁移重放两遍，所以 IF NOT EXISTS。
CREATE INDEX IF NOT EXISTS documents_sha_live_idx ON documents (sha256) WHERE purged_at IS NULL;
CREATE INDEX IF NOT EXISTS document_versions_sha_idx ON document_versions (sha256);
