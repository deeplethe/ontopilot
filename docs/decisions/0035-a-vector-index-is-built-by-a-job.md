# 0035 · A vector index is built by a job

- **Status**: implemented · a partial HNSW index per dimension on `chunks.embedding` and `entities.profile_embedding`, requested by the first write of that dimension and built by a `build_vector_index` job outside any transaction · `vector_search` and `nearest_typed_entities` write the dimension as a literal, cast both sides and set `hnsw.iterative_scan = relaxed_order` · type resolution gathers a batch's neighbours eight at a time, in order, and remembers descendant sets per batch (#512, #514) · dimensions above 2000 stay on the exact path
- **Written**: 2026-09-09 (conventions in the [README](README.md))
- **Related**: the ingest migration said P1 scans sequentially and an HNSW index comes "at volume"; this is that note coming due. [0019](0019-the-second-clock-can-be-rewound.md) is why the record-axis filter stays on the query and the index accommodates it. [0016](0016-close-the-open-seams-before-cutting-new-ones.md) C2 is the loop that scanned the entity table once per subject.

> `chunks.embedding` had no index of any kind, so every hybrid query computed cosine distance against every embedded chunk in the base and sorted the lot to take ten. `entities.profile_embedding` had none either, and type resolution asked it once per subject, sixty subjects a round, ten rounds a job. The column is `vector` with no `(N)`: the dimension follows the workspace's embedding model, which no migration knows.

## Measured before deciding

Sixty thousand chunks of 1024 dimensions in one table across three bases (50,000 / 10,000 / 20), random vectors, pgvector 0.8.6.

| | |
|---|---|
| Today's query, base of 50k | seq scan, 195 ms |
| Today's query, base of 10k | `chunks_kb_idx` + sort, 39 ms |
| Expression HNSW index, serial build over 60k rows | 87 s, 469 MB |
| Rewritten query, base of 50k | 4.5 ms |
| HNSW forced on the 10k base, `iterative_scan = off` | 3 rows of the 24 asked for |
| same, `relaxed_order` | 24 rows, 7.9 ms |

So the cost was about 4 ms per thousand chunks in the base being searched, per query. Nobody feels it at hundreds of chunks; a base of 100k pays 0.4 s per question on this leg, and type resolution paid it sixty times a round.

## Decisions

**A job, not a migration.** The dimension is only known when vectors are written, and `CREATE INDEX CONCURRENTLY` cannot run inside the transaction sqlx wraps migrations in, while a plain `CREATE INDEX` holds `ACCESS EXCLUSIVE` on `chunks` for the length of the build. So the first write of a dimension asks for an index (`vector_index::request`), which queues one `build_vector_index` job for that table and dimension unless one is already queued, and the job builds it `CONCURRENTLY IF NOT EXISTS` on a plain connection. An index that a failed build left invalid is dropped and rebuilt, because `IF NOT EXISTS` would otherwise take it for finished. Once the index is seen the process remembers it, so a write costs a set lookup; before that it costs one catalog query and one insert-unless-queued, for the minute or two the build takes.

**A partial expression index per dimension.** `USING hnsw ((embedding::vector(N)) vector_cosine_ops) WHERE vector_dims(embedding) = N`. The cast gives pgvector the dimension the column type lacks; the predicate keeps rows of another dimension out, so a workspace that changed its embedding model has two indexes and two populations rather than one build that fails. `vector_cosine_ops` matches the `<=>` the queries use.

**The dimension is a literal in the SQL.** With `vector_dims(embedding) = $2` bound as a parameter the custom plan uses the partial index and the generic plan falls back to a sequential scan. sqlx's prepared statements switch to a generic plan after five executions, so the index would have stopped being used on the sixth query, silently. `same_dims` and `distance` format the integer in, and the `ORDER BY` expression is character for character the indexed one.

**`hnsw.iterative_scan = relaxed_order` on every nearest-neighbour read.** HNSW takes `ef_search` candidates and applies the `WHERE` afterwards. On a table shared by tenants a small base holds few of those candidates, and `LIMIT 10` comes back with three rows or none: measured above, and the ordinary case rather than a corner. Iterative scan keeps walking until the limit is met or `hnsw.max_scan_tuples` (default 20,000) is reached; a base small enough to hit that ceiling is one the planner already routes to the exact path. The setting is `SET LOCAL`, so each read runs in a transaction. A pgvector older than 0.8 has no such setting; the process probes once and leaves it unset, and the query is still correct.

**The planner chooses the path.** With `chunks_kb_idx` present it picked index-plus-sort for the 20-row and 10k-row bases on its own and HNSW only for the 50k one. No application-side threshold: a threshold is a guess about the planner, and a wrong guess is slow on both sides.

**Entities take the same mechanism.** `nearest_typed_entities` reads the subject's vector first so the dimension can be written into the SQL, then runs the same shaped query. It gained a dimension guard it never had: a base with two dimensions of profiles used to error on `<=>`.

**The loop gathers before it reasons** (#514). The sixty neighbour queries of a batch are independent; they were serial because the loop was. `nearest_typed_for_each` runs them eight at a time with `buffered`, which yields in input order, and the per-subject reasoning stays in that order because adjudication downstream reads it. Eight is well under the pool of 32, which `db.rs` sizes for concurrent short queries. `descendants_of` is keyed on the coarse class, drawn from a small vocabulary, so a batch asks the same recursive query over and over; `DescendantsMemo` answers once per class per batch. A subject with no class has no descendants axis (0009) and stays out of the memo rather than sharing a key with a real class.

**Above 2000 dimensions there is no index.** That is the HNSW limit for `vector`; `text-embedding-3-large` is 3072. Such a dimension is never requested and the query stays exact. `halfvec` reaches 4000 at another precision and waits for someone to need it.

## Dead ends

- **ivfflat.** Needs a representative sample at build time to pick its lists, and a corpus that grows past the sample loses recall quietly. For a system whose claim is traceable evidence, silent recall loss is the wrong failure mode.
- **A numbered migration.** Two branches each adding the next number merge cleanly and then neither runs; and see the transaction above.
- **Dropping the record-axis filter so the index applies cleanly.** Replay is the product (0019). The index accommodates the filter.
- **Sizing the pool to the batch.** Sixty concurrent scans would fit a pool of 64 and move the load onto Postgres; `db.rs` already argues against sizing the pool to the worker count.
- **Caching neighbours across subjects.** Every subject has a different query vector; there is nothing to share.

## What the tests pin

Every question is asked twice, without the index and with it, and the answers must agree: the nearest chunk, a small base beside a large one filling its `LIMIT`, tenancy, a second dimension in one table, a superseded chunk, a moment on the record axis. For entities: the batch comes back in input order at concurrency 1 and 8, with and without the index; a two-connection pool completes sixty subjects; the memo returns what the query returns and does not notice a class added mid-batch. A test asserts on answers only; a plan change stays quiet.

## Open questions

- **Recall on a real corpus.** Random vectors are the worst case for ANN and say nothing about real embeddings; the acceptance item in #512, 200 real queries against the exact top-k, is the gate for trusting the index, and the figure belongs here once measured on a base large enough to make the planner choose the index. Raising `hnsw.ef_search` from 40 to 200 cost 10 ms in the synthetic run and is the first knob if the figure disappoints.
- **A dimension that leaves.** When a workspace changes model, the old dimension's index stays until someone drops it. It is small harm and no mechanism yet.
