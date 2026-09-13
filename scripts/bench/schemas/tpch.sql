-- TPC-H on Postgres：映射测量台的第一份语料（#501）
--
-- **数据不按 TPC-H 的生成规范来。** 那份规范值钱的地方是 schema 与 22 条查询——
-- 查询里写死了「收入」在这个 schema 上就是 sum(l_extendedprice * (1 - l_discount))，
-- 真值从那里抄，不是谁拍的脑袋。而行是怎么长出来的对打分毫无影响：探索只读
-- schema 不读数据，打分时 gold 与提议跑在同一批行上，比的是两个数一不一样。
-- 所以用 generate_series 造几万行，跑得快、装得下、可重放。
--
-- 表与列名照规范原样，包括 *_comment 那几列——它们是**数据列**（一段随机文本），
-- 与我们另外加的列注释是两回事。留着，因为真实库里也有这种名字唬人的列，
-- 探索会不会把 l_comment 当维度提出来是要量的事情之一。
--
-- 列注释单独在 tpch.comments.sql 里，不加载就是一份没有注释的同构语料。
-- 真实库里注释是模型最主要的线索，分开就能量出它值多少分。

DROP SCHEMA IF EXISTS tpch CASCADE;
CREATE SCHEMA tpch;
SET search_path TO tpch;

CREATE TABLE region (
    r_regionkey INTEGER PRIMARY KEY,
    r_name      CHAR(25) NOT NULL,
    r_comment   VARCHAR(152)
);

CREATE TABLE nation (
    n_nationkey INTEGER PRIMARY KEY,
    n_name      CHAR(25) NOT NULL,
    n_regionkey INTEGER NOT NULL REFERENCES region(r_regionkey),
    n_comment   VARCHAR(152)
);

CREATE TABLE supplier (
    s_suppkey   INTEGER PRIMARY KEY,
    s_name      CHAR(25) NOT NULL,
    s_address   VARCHAR(40) NOT NULL,
    s_nationkey INTEGER NOT NULL REFERENCES nation(n_nationkey),
    s_phone     CHAR(15) NOT NULL,
    s_acctbal   DECIMAL(15,2) NOT NULL,
    s_comment   VARCHAR(101)
);

CREATE TABLE customer (
    c_custkey    INTEGER PRIMARY KEY,
    c_name       VARCHAR(25) NOT NULL,
    c_address    VARCHAR(40) NOT NULL,
    c_nationkey  INTEGER NOT NULL REFERENCES nation(n_nationkey),
    c_phone      CHAR(15) NOT NULL,
    c_acctbal    DECIMAL(15,2) NOT NULL,
    c_mktsegment CHAR(10) NOT NULL,
    c_comment    VARCHAR(117)
);

CREATE TABLE part (
    p_partkey     INTEGER PRIMARY KEY,
    p_name        VARCHAR(55) NOT NULL,
    p_mfgr        CHAR(25) NOT NULL,
    p_brand       CHAR(10) NOT NULL,
    p_type        VARCHAR(25) NOT NULL,
    p_size        INTEGER NOT NULL,
    p_container   CHAR(10) NOT NULL,
    p_retailprice DECIMAL(15,2) NOT NULL,
    p_comment     VARCHAR(23)
);

CREATE TABLE partsupp (
    ps_partkey    INTEGER NOT NULL REFERENCES part(p_partkey),
    ps_suppkey    INTEGER NOT NULL REFERENCES supplier(s_suppkey),
    ps_availqty   INTEGER NOT NULL,
    ps_supplycost DECIMAL(15,2) NOT NULL,
    ps_comment    VARCHAR(199),
    PRIMARY KEY (ps_partkey, ps_suppkey)
);

CREATE TABLE orders (
    o_orderkey      INTEGER PRIMARY KEY,
    o_custkey       INTEGER NOT NULL REFERENCES customer(c_custkey),
    o_orderstatus   CHAR(1) NOT NULL,
    o_totalprice    DECIMAL(15,2) NOT NULL,
    o_orderdate     DATE NOT NULL,
    o_orderpriority CHAR(15) NOT NULL,
    o_clerk         CHAR(15) NOT NULL,
    o_shippriority  INTEGER NOT NULL,
    o_comment       VARCHAR(79)
);

CREATE TABLE lineitem (
    l_orderkey      INTEGER NOT NULL REFERENCES orders(o_orderkey),
    l_partkey       INTEGER NOT NULL,
    l_suppkey       INTEGER NOT NULL,
    l_linenumber    INTEGER NOT NULL,
    l_quantity      DECIMAL(15,2) NOT NULL,
    l_extendedprice DECIMAL(15,2) NOT NULL,
    l_discount      DECIMAL(15,2) NOT NULL,
    l_tax           DECIMAL(15,2) NOT NULL,
    l_returnflag    CHAR(1) NOT NULL,
    l_linestatus    CHAR(1) NOT NULL,
    l_shipdate      DATE NOT NULL,
    l_commitdate    DATE NOT NULL,
    l_receiptdate   DATE NOT NULL,
    l_shipinstruct  CHAR(25) NOT NULL,
    l_shipmode      CHAR(10) NOT NULL,
    l_comment       VARCHAR(44),
    PRIMARY KEY (l_orderkey, l_linenumber),
    FOREIGN KEY (l_partkey, l_suppkey) REFERENCES partsupp(ps_partkey, ps_suppkey)
);

-- ---------------------------------------------------------------- 行
--
-- setseed 让同一份语料每次生成得一模一样：**两轮探索的分数要可比**，
-- 语料自己先不能变（bench/README 那条「每一组一个新库」的同一个理由）。

SELECT setseed(0.42);

INSERT INTO region VALUES
  (0, 'AFRICA',      'lar deposits. blithely final packages cajole'),
  (1, 'AMERICA',     'hs use ironic, even requests'),
  (2, 'ASIA',        'ges. thinly even pinto beans ca'),
  (3, 'EUROPE',      'ly final courts cajole furiously final excuse'),
  (4, 'MIDDLE EAST', 'uickly special accounts cajole carefully blithely close requests');

INSERT INTO nation (n_nationkey, n_name, n_regionkey, n_comment)
SELECT * FROM (VALUES
  (0,'ALGERIA',0),(1,'ARGENTINA',1),(2,'BRAZIL',1),(3,'CANADA',1),(4,'EGYPT',4),
  (5,'ETHIOPIA',0),(6,'FRANCE',3),(7,'GERMANY',3),(8,'INDIA',2),(9,'INDONESIA',2),
  (10,'IRAN',4),(11,'IRAQ',4),(12,'JAPAN',2),(13,'JORDAN',4),(14,'KENYA',0),
  (15,'MOROCCO',0),(16,'MOZAMBIQUE',0),(17,'PERU',1),(18,'CHINA',2),(19,'ROMANIA',3),
  (20,'SAUDI ARABIA',4),(21,'VIETNAM',2),(22,'RUSSIA',3),(23,'UNITED KINGDOM',3),
  (24,'UNITED STATES',1)
) AS v(k, n, r), LATERAL (SELECT 'final accounts wake ' || n) AS c(cm);

INSERT INTO supplier
SELECT i,
       'Supplier#' || lpad(i::text, 9, '0'),
       lpad(md5(i::text), 20, 'x'),
       (random() * 24)::int,
       '27-' || lpad(((random() * 899 + 100))::int::text, 3, '0') || '-' ||
                lpad(((random() * 899 + 100))::int::text, 3, '0') || '-' ||
                lpad(((random() * 8999 + 1000))::int::text, 4, '0'),
       round((random() * 10000 - 1000)::numeric, 2),
       'each slyly above the careful'
FROM generate_series(1, 100) AS i;

INSERT INTO customer
SELECT i,
       'Customer#' || lpad(i::text, 9, '0'),
       lpad(md5(i::text || 'a'), 25, 'y'),
       (random() * 24)::int,
       '25-' || lpad(((random() * 899 + 100))::int::text, 3, '0') || '-' ||
                lpad(((random() * 899 + 100))::int::text, 3, '0') || '-' ||
                lpad(((random() * 8999 + 1000))::int::text, 4, '0'),
       round((random() * 10000 - 1000)::numeric, 2),
       (ARRAY['AUTOMOBILE','BUILDING','FURNITURE','MACHINERY','HOUSEHOLD'])[1 + (random() * 4)::int],
       'requests wake fluffily'
FROM generate_series(1, 1500) AS i;

INSERT INTO part
SELECT i,
       (ARRAY['almond','antique','blush','burnished','cornflower'])[1 + (random() * 4)::int] || ' ' ||
       (ARRAY['azure','chocolate','dim','frosted','ghost'])[1 + (random() * 4)::int] || ' ' ||
       (ARRAY['lace','metallic','powder','rose','steel'])[1 + (random() * 4)::int],
       'Manufacturer#' || (1 + (random() * 4)::int),
       'Brand#' || (1 + (random() * 4)::int) || (1 + (random() * 4)::int),
       (ARRAY['STANDARD','SMALL','MEDIUM','LARGE','ECONOMY','PROMO'])[1 + (random() * 5)::int] || ' ' ||
       (ARRAY['ANODIZED','BURNISHED','PLATED','POLISHED','BRUSHED'])[1 + (random() * 4)::int] || ' ' ||
       (ARRAY['TIN','NICKEL','BRASS','STEEL','COPPER'])[1 + (random() * 4)::int],
       1 + (random() * 49)::int,
       (ARRAY['SM','LG','MED','JUMBO','WRAP'])[1 + (random() * 4)::int] || ' ' ||
       (ARRAY['CASE','BOX','BAG','JAR','PKG'])[1 + (random() * 4)::int],
       round((90 + i % 2000 * 0.5 + random() * 10)::numeric, 2),
       'final deposits'
FROM generate_series(1, 2000) AS i;

-- 每个零件四个供应商（规范是 SUPPLIER_PER_PART = 4）
INSERT INTO partsupp
SELECT p.p_partkey,
       1 + ((p.p_partkey * 7 + s.n * 23) % 100),
       (random() * 9999)::int,
       round((random() * 1000 + 1)::numeric, 2),
       'careful accounts sleep'
FROM part p CROSS JOIN generate_series(0, 3) AS s(n)
ON CONFLICT DO NOTHING;

INSERT INTO orders
SELECT i,
       1 + (random() * 1499)::int,
       'O',
       0,                                            -- 明细生成完再回填
       DATE '1992-01-01' + (random() * 2520)::int,   -- 1992-01-01 .. 1998-12-31
       (ARRAY['1-URGENT','2-HIGH','3-MEDIUM','4-NOT SPECIFIED','5-LOW'])[1 + (random() * 4)::int],
       'Clerk#' || lpad((1 + (random() * 99)::int)::text, 9, '0'),
       0,
       'slyly special requests'
FROM generate_series(1, 15000) AS i;

-- 明细。**数值关系要立得住**：l_extendedprice = l_quantity × p_retailprice，
-- 折扣 0–0.10、税 0–0.08，都照规范的取值域——Q1 的 disc_price 与 charge
-- 两条口径全建在这三列上，随便填的话真值算出来的数没有意义。
--
-- returnflag / linestatus 按 shipdate 定，也是规范的规则：1995-06-17 之前
-- 发的是已结（F，退货 R 或已收 A），之后是在途（O，N）。Q1 正是按这两列分组。
INSERT INTO lineitem
WITH ps AS (
    SELECT row_number() OVER (ORDER BY ps_partkey, ps_suppkey) AS rn, ps_partkey, ps_suppkey
    FROM partsupp
),
n AS (SELECT count(*) AS c FROM ps),
raw AS (
    SELECT o.o_orderkey,
           o.o_orderdate,
           ln::int AS l_linenumber,
           1 + (random() * (n.c - 1))::int AS ps_rn,
           round((1 + random() * 49)::numeric, 2) AS l_quantity,
           round((random() * 0.10)::numeric, 2) AS l_discount,
           round((random() * 0.08)::numeric, 2) AS l_tax,
           o.o_orderdate + (1 + random() * 120)::int AS l_shipdate
    FROM orders o
    CROSS JOIN n
    CROSS JOIN LATERAL generate_series(1, 1 + (random() * 5)::int) AS ln
)
SELECT r.o_orderkey, ps.ps_partkey, ps.ps_suppkey, r.l_linenumber,
       r.l_quantity,
       round(r.l_quantity * p.p_retailprice, 2),
       r.l_discount,
       r.l_tax,
       CASE WHEN r.l_shipdate <= DATE '1995-06-17'
            THEN (ARRAY['R','A'])[1 + (random() * 1)::int] ELSE 'N' END,
       CASE WHEN r.l_shipdate <= DATE '1995-06-17' THEN 'F' ELSE 'O' END,
       r.l_shipdate,
       r.o_orderdate + (1 + random() * 90)::int,
       r.l_shipdate + (1 + random() * 30)::int,
       (ARRAY['DELIVER IN PERSON','COLLECT COD','NONE','TAKE BACK RETURN'])[1 + (random() * 3)::int],
       (ARRAY['REG AIR','AIR','RAIL','SHIP','TRUCK','MAIL','FOB'])[1 + (random() * 6)::int],
       'carefully ironic deposits'
FROM raw r
JOIN ps ON ps.rn = r.ps_rn
JOIN part p ON p.p_partkey = ps.ps_partkey;

-- 订单总额回填：含税折后价之和，与规范一致。o_totalprice 与 lineitem
-- 对得上，「订单均价」这类口径才有唯一答案
UPDATE orders o
   SET o_totalprice = t.total,
       o_orderstatus = t.status
  FROM (
    SELECT l_orderkey,
           round(sum(l_extendedprice * (1 - l_discount) * (1 + l_tax)), 2) AS total,
           CASE WHEN count(*) FILTER (WHERE l_linestatus = 'O') = 0 THEN 'F'
                WHEN count(*) FILTER (WHERE l_linestatus = 'F') = 0 THEN 'O'
                ELSE 'P' END AS status
      FROM lineitem GROUP BY l_orderkey
  ) t
 WHERE o.o_orderkey = t.l_orderkey;

ANALYZE;
