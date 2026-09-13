//! Postgres 族（顺带覆盖 Greenplum / Timescale 等 PG 兼容系）。线协议直连，
//! 是四个引擎里唯一有会话可设只读的那个。

use super::{QueryEngine, QueryResult, SchemaColumn, ROW_CAP, STATEMENT_TIMEOUT_SECS};
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use std::time::Duration;

pub struct PostgresEngine {
    conn: String,
}

impl PostgresEngine {
    pub fn new(conn: &str) -> Self {
        Self {
            conn: conn.to_string(),
        }
    }

    async fn pool(&self) -> anyhow::Result<sqlx::PgPool> {
        Ok(PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&self.conn)
            .await?)
    }
}

#[async_trait::async_trait]
impl QueryEngine for PostgresEngine {
    async fn test(&self) -> anyhow::Result<()> {
        let pool = self.pool().await?;
        sqlx::query("SELECT 1").execute(&pool).await?;
        pool.close().await;
        Ok(())
    }

    async fn fetch_schema(&self) -> anyhow::Result<Vec<SchemaColumn>> {
        let pool = self.pool().await?;
        // 三块信息三段 SQL。
        //
        // 1) 列 + 类型 + 注释（原本就是这块）。
        // 2) 单列主键（`information_schema.table_constraints` 过滤 PRIMARY KEY，
        //    `key_column_usage` 取到具体那一列）。组合主键故意不展开——
        //    `explore_mappings` 的提示词读 `is_primary_key=true` 当 ID 用，
        //    把 3/5 列的组合标成 true 反而误导。
        // 3) 外键（`referential_constraints` + `key_column_usage` +
        //    `constraint_column_usage`）：FK 也只看该列本身是 FK 的情况，
        //    跨列的组合外键同样不展开。
        // 4) 可空：`information_schema.columns.is_nullable`。PK 列构造上
        //    一定 NOT NULL，但 SQL 不替我们保证——查出来最稳。
        //
        // 三段 SQL 都按 `(table_schema, table_name, column_name)` 排序，
        // 客户端按这个顺序合到 `columns` 上（PG 已经返回的也是这个序）
        let cols: Vec<(
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
        )> = sqlx::query_as(
            "SELECT c.table_schema, c.table_name, c.column_name,
                    c.data_type, c.is_nullable,
                    pgd.description
             FROM information_schema.columns c
             LEFT JOIN pg_catalog.pg_statio_all_tables st
               ON st.schemaname = c.table_schema AND st.relname = c.table_name
             LEFT JOIN pg_catalog.pg_description pgd
               ON pgd.objoid = st.relid AND pgd.objsubid = c.ordinal_position
             WHERE c.table_schema NOT IN ('pg_catalog', 'information_schema')
             ORDER BY c.table_schema, c.table_name, c.ordinal_position",
        )
        .fetch_all(&pool)
        .await?;
        let pks: Vec<(String, String, String)> = sqlx::query_as(
            // 单列主键：约束名对应的列数 = 1 的那一条。组合主键里那一列
            // `ordinal_position=1` 也满足不了「列数 = 1」这个条件，所以不会被标
            // 成 is_primary_key。explore_mappings 的提示词读 `is_primary_key=true`
            // 当 ID 用，把组合 PK 的两列都标 true 反而误导（#502）
            "SELECT kcu.table_schema, kcu.table_name, kcu.column_name
             FROM information_schema.table_constraints tc
             JOIN information_schema.key_column_usage kcu
               ON tc.constraint_schema = kcu.constraint_schema
              AND tc.constraint_name = kcu.constraint_name
             WHERE tc.constraint_type = 'PRIMARY KEY'
               AND tc.table_schema NOT IN ('pg_catalog', 'information_schema')
               AND (
                 SELECT count(*) FROM information_schema.key_column_usage kcu2
                 WHERE kcu2.constraint_schema = tc.constraint_schema
                   AND kcu2.constraint_name  = tc.constraint_name
               ) = 1",
        )
        .fetch_all(&pool)
        .await?;
        let fks: Vec<(String, String, String, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT kcu.table_schema, kcu.table_name, kcu.column_name,
                    ccu.table_schema, ccu.table_name
             FROM information_schema.table_constraints tc
             JOIN information_schema.key_column_usage kcu
               ON tc.constraint_schema = kcu.constraint_schema
              AND tc.constraint_name = kcu.constraint_name
             JOIN information_schema.referential_constraints rc
               ON tc.constraint_schema = rc.constraint_schema
              AND tc.constraint_name = rc.constraint_name
             JOIN information_schema.constraint_column_usage ccu
               ON rc.unique_constraint_schema = ccu.constraint_schema
              AND rc.unique_constraint_name = ccu.constraint_name
             WHERE tc.constraint_type = 'FOREIGN KEY'
               AND tc.table_schema NOT IN ('pg_catalog', 'information_schema')",
        )
        .fetch_all(&pool)
        .await?;
        pool.close().await;

        // 三段 SQL 各自按 (schema, table, column) 排序——线性合并成一个 HashMap
        // 反而不如两个集合（PK set + FK map）查起来直接
        use std::collections::{HashMap, HashSet};
        let mut pk_set: HashSet<(String, String, String)> = HashSet::with_capacity(pks.len());
        for (s, t, c) in &pks {
            pk_set.insert((s.clone(), t.clone(), c.clone()));
        }
        let mut fk_map: HashMap<(String, String, String), Option<String>> =
            HashMap::with_capacity(fks.len());
        for (s, t, c, ref_schema, ref_table) in &fks {
            let target = match (ref_schema, ref_table) {
                (Some(rs), Some(rt)) => Some(format!("{rs}.{rt}")),
                _ => None,
            };
            fk_map.insert((s.clone(), t.clone(), c.clone()), target);
        }

        Ok(cols
            .into_iter()
            .map(|(schema, table, column, data_type, is_nullable, comment)| {
                let key = (schema.clone(), table.clone(), column.clone());
                let is_primary_key = pk_set.contains(&key);
                let (is_foreign_key, references_table) = match fk_map.get(&key) {
                    Some(target) => (true, target.clone()),
                    None => (false, None),
                };
                let nullable = matches!(is_nullable.as_deref(), Some("YES"));
                SchemaColumn {
                    schema,
                    table,
                    column,
                    data_type,
                    comment,
                    is_primary_key,
                    is_foreign_key,
                    references_table,
                    nullable,
                }
            })
            .collect())
    }

    async fn execute(&self, sql: &str) -> anyhow::Result<QueryResult> {
        let pool = self.pool().await?;
        // 纵深防御第 3 层：会话级只读 + 超时（parser 漏网也写不进去、跑不死库）
        sqlx::query("SET default_transaction_read_only = on")
            .execute(&pool)
            .await?;
        sqlx::query(&format!(
            "SET statement_timeout = '{STATEMENT_TIMEOUT_SECS}s'"
        ))
        .execute(&pool)
        .await?;
        // 第 2 层：外包 LIMIT；row_to_json 让 PG 全权处理类型→JSON（文本键序保留列序）
        let wrapped = format!(
            "SELECT row_to_json(_q)::text AS _j FROM ( {sql} ) AS _q LIMIT {}",
            ROW_CAP + 1
        );
        let fetched = sqlx::query(&wrapped).fetch_all(&pool).await?;
        pool.close().await;

        let truncated = fetched.len() > ROW_CAP;
        let rows = fetched
            .into_iter()
            .take(ROW_CAP)
            .map(|r| r.try_get::<String, _>("_j").unwrap_or_else(|_| "{}".into()))
            .collect();
        Ok(QueryResult { rows, truncated })
    }
}

#[cfg(test)]
mod tests {
    use super::{PostgresEngine, QueryEngine};
    use sqlx::postgres::PgPoolOptions;
    use std::sync::OnceLock;
    use tokio::sync::Mutex;

    /// 对着真服务器跑的那一档。`UTOPIA_TEST_DATABASE_URL` 没设就跳过——
    /// 组合主键、跨 schema 外键、`is_nullable='NO'` 这些细节只有连真库才见得到
    fn live_url() -> Option<String> {
        std::env::var("UTOPIA_TEST_DATABASE_URL")
            .or_else(|_| std::env::var("UTOPIA_DATABASE_URL"))
            .ok()
            .filter(|u| !u.trim().is_empty())
    }

    /// 起一个最小可用的测试 schema：`serial` / `int` 不需要任何扩展，
    /// 组合主键 / 单列 PK / FK / 可空对照这里都覆盖了
    ///
    /// 三件并行会撞的事：每个 `CREATE TABLE` 都往 `pg_class` 写一行，
    /// 三个测试同时跑就在那里打架。OnceLock + Mutex 让三个测试排队建表：
    /// 一号测试建完之后二号测试直接复用同一份 schema，不再各自 CREATE。
    /// `schema_keys_*` 是测试专属前缀，跟应用表（`chunks` / `concepts` ...）不会撞
    async fn setup_schema(pool: &sqlx::PgPool) -> anyhow::Result<()> {
        sqlx::query("DROP TABLE IF EXISTS schema_keys_child")
            .execute(pool)
            .await?;
        sqlx::query("DROP TABLE IF EXISTS schema_keys_parent")
            .execute(pool)
            .await?;
        sqlx::query(
            "CREATE TABLE schema_keys_parent (
                 id serial PRIMARY KEY,
                 name text NOT NULL,
                 -- 单列 PK + 单列 FK 各一，用来确认两条查询都拿到了
                 ref_parent int REFERENCES schema_keys_parent(id),
                 -- 普通列，留作 nullable 的对照
                 note text
             )",
        )
        .execute(pool)
        .await?;
        sqlx::query(
            "CREATE TABLE schema_keys_child (
                 -- 组合主键（id, ordinal）里这一列 NOT NULL 但不是「单列主键」，
                 -- fetch_schema 必须把它判成 false 才对
                 parent_id int NOT NULL REFERENCES schema_keys_parent(id),
                 ordinal int NOT NULL,
                 payload text,
                 PRIMARY KEY (parent_id, ordinal)
             )",
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// 三件并行争 `pg_class_relname_nsp_index`：用一个全局 Mutex 串起来。
    /// tokio 测试本来在同进程多线程里跑，没这个锁就把 CREATE TABLE 撞穿
    static SCHEMA_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    fn lock() -> &'static Mutex<()> {
        SCHEMA_LOCK.get_or_init(|| Mutex::new(()))
    }

    #[tokio::test]
    async fn a_primary_key_column_is_marked_and_not_null() {
        let Some(url) = live_url() else { return };
        let _g = lock().lock().await;
        let engine = PostgresEngine::new(&url);
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("connect");
        setup_schema(&pool).await.expect("setup");

        let cols = engine.fetch_schema().await.expect("schema");
        let id = cols
            .iter()
            .find(|c| c.table == "schema_keys_parent" && c.column == "id")
            .expect("parent.id");
        assert!(id.is_primary_key, "id 是单列 PK");
        assert!(!id.nullable, "PK 必 NOT NULL");
        assert!(!id.is_foreign_key, "PK 不是 FK");

        // 普通可空列：nullable=true，其他标志都是 false
        let note = cols
            .iter()
            .find(|c| c.table == "schema_keys_parent" && c.column == "note")
            .expect("parent.note");
        assert!(note.nullable, "note 没 NOT NULL 约束");
        assert!(!note.is_primary_key);
        assert!(!note.is_foreign_key);

        pool.close().await;
    }

    #[tokio::test]
    async fn a_single_column_foreign_key_carries_its_target() {
        let Some(url) = live_url() else { return };
        let _g = lock().lock().await;
        let engine = PostgresEngine::new(&url);
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("connect");
        setup_schema(&pool).await.expect("setup");

        let cols = engine.fetch_schema().await.expect("schema");
        // parent.ref_parent 是单列 FK
        let refp = cols
            .iter()
            .find(|c| c.table == "schema_keys_parent" && c.column == "ref_parent")
            .expect("parent.ref_parent");
        assert!(refp.is_foreign_key);
        assert!(!refp.is_primary_key, "FK 不是 PK");
        assert_eq!(
            refp.references_table.as_deref(),
            Some("public.schema_keys_parent"),
            "FK 应该指回自己（自引用）"
        );

        // child.parent_id 也是 FK，目标是 parent
        let child_fk = cols
            .iter()
            .find(|c| c.table == "schema_keys_child" && c.column == "parent_id")
            .expect("child.parent_id");
        assert!(child_fk.is_foreign_key);
        assert_eq!(
            child_fk.references_table.as_deref(),
            Some("public.schema_keys_parent")
        );
        assert!(!child_fk.nullable, "FK 列是 NOT NULL");

        pool.close().await;
    }

    #[tokio::test]
    async fn a_composite_primary_key_member_stays_unmarked() {
        let Some(url) = live_url() else { return };
        let _g = lock().lock().await;
        let engine = PostgresEngine::new(&url);
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("connect");
        setup_schema(&pool).await.expect("setup");

        let cols = engine.fetch_schema().await.expect("schema");
        // (parent_id, ordinal) 是组合主键——两列都不该被标成 is_primary_key
        for col_name in ["parent_id", "ordinal"] {
            let c = cols
                .iter()
                .find(|c| c.table == "schema_keys_child" && c.column == col_name)
                .unwrap_or_else(|| panic!("child.{col_name}"));
            assert!(
                !c.is_primary_key,
                "child.{col_name} 是组合主键的一员，不应标成单列 PK"
            );
            // 两列都是 NOT NULL（组合 PK 必需）
            assert!(
                !c.nullable,
                "child.{col_name} 是组合 PK 的一员，必 NOT NULL"
            );
        }

        pool.close().await;
    }
}
