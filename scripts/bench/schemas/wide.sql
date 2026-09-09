-- 一张打平的电商订单宽表：映射测量台的第二份语料（#501 / #520）。
--
-- **TPC-H 量不出语义层是干什么的。** 八张表、诚实的列名、每个分析列一条注释，
-- 模型光看 schema 文档就答对 24 题里的 23 题，语义层只剩一题的空间（#520 第一轮）。
-- 那不是语义层没用，是那份 schema 自己把答案写在了列名上。
--
-- 这一份反过来造：**每一条口径都配一个「模型会自然猜错」的做法**。
--
-- 1. **金额单位是分。** `amt_pay` 是 5980 而不是 59.80。天真的 `sum(amt_pay)`
--    跑得通、数好看、错一百倍。
-- 2. **列名是内部黑话。** `amt_*` / `dt_*` / `ord_st` / `chnl` / `flg_*`——
--    真实数仓的样子，而不是 `l_extendedprice`。
-- 3. **口径是业务约定，不在列名里。** 「GMV」要排掉测试单、只算已支付；
--    「有效订单」有一张状态白名单；「净销售额」要扣退款。这些约定 schema
--    一个字都没写，一个没被告知的人（或模型）不可能猜对。
-- 4. **相似列成对出现。** `amt_total`（含运费与优惠前）与 `amt_pay`（实付）
--    差着运费和优惠，选错一个数就错，而两个都跑得通。
-- 5. **陷阱列。** 整数外键 sum 得动；`ver` 恒为 1；`price_old` 是废弃列全 NULL；
--    `is_test` 求和是「测试单数」，看着像个指标。
--
-- join 打平是宽表的定义，也是它没有外键的原因——#502 的 `fetch_keys` 在这种
-- 表上一无所获，只有抽样与基数分得出 `buyer_id` 是键而 `qty` 是数。

DROP SCHEMA IF EXISTS dw CASCADE;
CREATE SCHEMA dw;
SET search_path TO dw;

-- 订单明细宽表。一行 = 一个订单里的一个商品行
CREATE TABLE dwd_ord_dtl (
    -- 键。**全是整数，全都 sum 得动**
    id          BIGSERIAL PRIMARY KEY,
    ord_id      BIGINT NOT NULL,
    ord_ln      SMALLINT NOT NULL,
    buyer_id    BIGINT NOT NULL,
    seller_id   BIGINT NOT NULL,
    shop_id     INTEGER NOT NULL,
    item_id     BIGINT NOT NULL,
    sku_id      BIGINT NOT NULL,
    cat_id      INTEGER NOT NULL,
    cat_id_l1   INTEGER NOT NULL,
    addr_id     BIGINT NOT NULL,
    pay_id      BIGINT,
    promo_id    INTEGER,
    cpn_id      INTEGER,
    lgst_id     BIGINT,

    -- 金额，**单位是分**。天真的 sum 会给出一个大一百倍的数
    amt_total   BIGINT NOT NULL,
    amt_pay     BIGINT NOT NULL,
    amt_item    BIGINT NOT NULL,
    amt_frght   BIGINT NOT NULL,
    amt_disc    BIGINT NOT NULL,
    amt_cpn     BIGINT NOT NULL,
    amt_rfnd    BIGINT NOT NULL,
    amt_tax     BIGINT NOT NULL,
    amt_cost    BIGINT NOT NULL,
    amt_pt      BIGINT NOT NULL,
    -- 废弃列：迁移之后再没写过，全是 NULL
    price_old   BIGINT,

    qty         INTEGER NOT NULL,
    qty_rfnd    INTEGER NOT NULL,
    wt_g        INTEGER,

    -- 状态与标志。**口径的白名单藏在这里，而 schema 不说**
    ord_st      SMALLINT NOT NULL,
    pay_st      SMALLINT NOT NULL,
    lgst_st     SMALLINT NOT NULL,
    rfnd_st     SMALLINT NOT NULL,
    is_test     SMALLINT NOT NULL,
    is_gift     SMALLINT NOT NULL,
    is_presale  SMALLINT NOT NULL,
    is_1st      SMALLINT NOT NULL,
    flg_r       SMALLINT NOT NULL,
    chnl        SMALLINT NOT NULL,
    pay_mtd     SMALLINT NOT NULL,

    -- 时间
    dt_crt      TIMESTAMPTZ NOT NULL,
    dt_pay      TIMESTAMPTZ,
    dt_shp      TIMESTAMPTZ,
    dt_fin      TIMESTAMPTZ,
    dt_rfnd     TIMESTAMPTZ,
    stat_dt     DATE NOT NULL,

    -- 打平进来的维度
    shop_nm     VARCHAR(60) NOT NULL,
    cat_nm      VARCHAR(40) NOT NULL,
    cat_nm_l1   VARCHAR(40) NOT NULL,
    item_nm     VARCHAR(80) NOT NULL,
    brand_nm    VARCHAR(40),
    prov        VARCHAR(20) NOT NULL,
    city        VARCHAR(30) NOT NULL,
    buyer_lvl   SMALLINT NOT NULL,
    src         VARCHAR(20) NOT NULL,
    dev         VARCHAR(20),

    -- 噪声：恒定、自由文本、ETL 元数据
    ver         SMALLINT NOT NULL DEFAULT 1,
    rmk         VARCHAR(200),
    ext         VARCHAR(200),
    etl_dt      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 店铺维表。**只有它有一条外键关系**，而宽表这边没有约束声明——
-- 打平之后 shop_id 与 shop_nm 都在事实表里，维表是留着的那份原始记录
CREATE TABLE dim_shop (
    shop_id   INTEGER PRIMARY KEY,
    shop_nm   VARCHAR(60) NOT NULL,
    seller_id BIGINT NOT NULL,
    prov      VARCHAR(20) NOT NULL,
    lvl       SMALLINT NOT NULL,
    open_dt   DATE NOT NULL
);
