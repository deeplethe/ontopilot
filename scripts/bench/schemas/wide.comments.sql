-- `wide` 的列注释——**写成真实数仓里那种注释**，而不是 TPC-H 那种教科书注释。
--
-- 真实的宽表注释有三个特点，这里都照做：
--
-- 1. **只有一部分列有。** 建表的人给自己关心的写了，剩下的没写。
-- 2. **说的是字段是什么，不是口径是什么。** 「实付金额（分）」是列的事实；
--    「GMV 要排掉测试单且只算已支付」是业务约定，它住在文档、周会和某个人
--    脑子里，不住在 `COMMENT ON COLUMN` 里。
-- 3. **状态码列出取值，但不说哪些算数。** 「1 创建 2 已付 3 已发 4 完成
--    5 关闭 6 退款」——看得见六个值，看不出「有效订单是 2/3/4」。
--
-- 所以带上这份注释之后，模型该能答对单位，仍然答不对口径。**那正是语义层
-- 要填的那一格**：注释解决列的歧义，口径解决业务的歧义，两件事。

SET search_path TO dw;

COMMENT ON TABLE dwd_ord_dtl IS '订单明细宽表，一行一个订单商品行，T+1 由 ods 层打平生成';

COMMENT ON COLUMN dwd_ord_dtl.ord_id     IS '订单号';
COMMENT ON COLUMN dwd_ord_dtl.ord_ln     IS '订单内行号';
COMMENT ON COLUMN dwd_ord_dtl.buyer_id   IS '买家 ID';
COMMENT ON COLUMN dwd_ord_dtl.shop_id    IS '店铺 ID，关联 dim_shop';
COMMENT ON COLUMN dwd_ord_dtl.item_id    IS '商品 ID';
COMMENT ON COLUMN dwd_ord_dtl.cat_id     IS '叶子类目 ID';

COMMENT ON COLUMN dwd_ord_dtl.amt_total  IS '订单金额（分），商品金额 + 运费，优惠前';
COMMENT ON COLUMN dwd_ord_dtl.amt_pay    IS '实付金额（分）';
COMMENT ON COLUMN dwd_ord_dtl.amt_item   IS '商品金额（分），单价 × 件数';
COMMENT ON COLUMN dwd_ord_dtl.amt_frght  IS '运费（分）';
COMMENT ON COLUMN dwd_ord_dtl.amt_disc   IS '活动优惠（分）';
COMMENT ON COLUMN dwd_ord_dtl.amt_cpn    IS '优惠券抵扣（分）';
COMMENT ON COLUMN dwd_ord_dtl.amt_rfnd   IS '退款金额（分）';
COMMENT ON COLUMN dwd_ord_dtl.amt_cost   IS '商品成本（分）';
COMMENT ON COLUMN dwd_ord_dtl.amt_pt     IS '积分抵扣（积分数，非金额）';
COMMENT ON COLUMN dwd_ord_dtl.price_old  IS '旧价格字段，2023 年迁移后不再写入';

COMMENT ON COLUMN dwd_ord_dtl.qty        IS '件数';
COMMENT ON COLUMN dwd_ord_dtl.qty_rfnd   IS '退货件数';

COMMENT ON COLUMN dwd_ord_dtl.ord_st     IS '订单状态：1 创建 2 已付 3 已发货 4 已完成 5 已关闭 6 已退款';
COMMENT ON COLUMN dwd_ord_dtl.pay_st     IS '支付状态：0 未支付 1 已支付 2 部分退款 3 全额退款';
COMMENT ON COLUMN dwd_ord_dtl.is_test    IS '测试单标记';
COMMENT ON COLUMN dwd_ord_dtl.is_1st     IS '是否该买家首单';
COMMENT ON COLUMN dwd_ord_dtl.flg_r      IS '退货标记';
COMMENT ON COLUMN dwd_ord_dtl.chnl       IS '渠道：1 APP 2 站内 H5 3 小程序 4 三方平台';
COMMENT ON COLUMN dwd_ord_dtl.pay_mtd    IS '支付方式：1 支付宝 2 微信 3 银行卡 4 其他';

COMMENT ON COLUMN dwd_ord_dtl.dt_crt     IS '下单时间';
COMMENT ON COLUMN dwd_ord_dtl.dt_pay     IS '支付时间';
COMMENT ON COLUMN dwd_ord_dtl.stat_dt    IS '统计日期（分区字段），取下单日期';
COMMENT ON COLUMN dwd_ord_dtl.etl_dt     IS 'ETL 写入时间';

COMMENT ON COLUMN dwd_ord_dtl.shop_nm    IS '店铺名称';
COMMENT ON COLUMN dwd_ord_dtl.cat_nm     IS '叶子类目名称';
COMMENT ON COLUMN dwd_ord_dtl.cat_nm_l1  IS '一级类目名称';
COMMENT ON COLUMN dwd_ord_dtl.prov       IS '收货省份';
COMMENT ON COLUMN dwd_ord_dtl.city       IS '收货城市';
COMMENT ON COLUMN dwd_ord_dtl.buyer_lvl  IS '买家等级 1-5';
COMMENT ON COLUMN dwd_ord_dtl.ver        IS 'schema 版本号';

COMMENT ON TABLE dim_shop IS '店铺维表';
COMMENT ON COLUMN dim_shop.shop_nm IS '店铺名称';
COMMENT ON COLUMN dim_shop.lvl     IS '店铺等级 1-5';
