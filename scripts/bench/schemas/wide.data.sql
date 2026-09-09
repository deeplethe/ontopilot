-- `wide` 语料的行。DDL 在 wide.sql，两者一起加载。
--
-- setseed 固定：**两轮的分数要可比，语料自己先不能变**。

SET search_path TO dw;
SELECT setseed(0.73);

INSERT INTO dim_shop
SELECT i,
       (ARRAY['京东自营','天猫旗舰','拼多多','唯品会','苏宁','小米','华为','安踏'])[1 + (random()*7)::int]
         || (ARRAY['旗舰店','专营店','官方店','工厂店'])[1 + (random()*3)::int] || i,
       100000 + (random()*899)::bigint,
       (ARRAY['广东','浙江','江苏','上海','北京','山东','四川','福建'])[1 + (random()*7)::int],
       1 + (random()*4)::int,
       DATE '2019-01-01' + (random()*2000)::int
FROM generate_series(1, 200) AS i;

INSERT INTO dwd_ord_dtl (
    ord_id, ord_ln, buyer_id, seller_id, shop_id, item_id, sku_id, cat_id, cat_id_l1,
    addr_id, pay_id, promo_id, cpn_id, lgst_id,
    amt_total, amt_pay, amt_item, amt_frght, amt_disc, amt_cpn, amt_rfnd, amt_tax,
    amt_cost, amt_pt, price_old, qty, qty_rfnd, wt_g,
    ord_st, pay_st, lgst_st, rfnd_st, is_test, is_gift, is_presale, is_1st, flg_r,
    chnl, pay_mtd, dt_crt, dt_pay, dt_shp, dt_fin, dt_rfnd, stat_dt,
    shop_nm, cat_nm, cat_nm_l1, item_nm, brand_nm, prov, city, buyer_lvl, src, dev,
    ver, rmk, ext
)
WITH base AS (
    SELECT i,
           (i - 1) / 2 + 1                              AS ord_id,
           ((i - 1) % 2 + 1)::smallint                  AS ord_ln,
           1 + (random() * 4999)::int                   AS buyer_id,
           1 + (random() * 199)::int                    AS shop_id,
           1 + (random() * 49)::int                     AS cat_id,
           1 + (random() * 4)::int                      AS qty,
           1000 + (random() * 49000)::bigint            AS unit_price,
           random()                                     AS r_test,
           random()                                     AS r_pay,
           random()                                     AS r_disc,
           random()                                     AS r_cpn,
           random()                                     AS r_rfnd,
           random()                                     AS r_chnl,
           random()                                     AS r_1st,
           random()                                     AS r_misc,
           DATE '2025-01-01' + (random() * 364)::int    AS d_crt
    FROM generate_series(1, 60000) AS i
),
money AS (
    SELECT *,
           unit_price * qty                                                    AS amt_item,
           (unit_price * qty * r_disc * 0.2)::bigint                           AS amt_disc,
           CASE WHEN r_cpn < 0.3 THEN (500 + r_cpn * 6000)::bigint ELSE 0 END  AS amt_cpn,
           -- 满 99 元包邮：一条真实的业务规则，schema 里同样一个字都没有
           CASE WHEN unit_price * qty >= 9900 THEN 0
                ELSE (500 + r_misc * 1000)::bigint END                         AS amt_frght,
           CASE WHEN r_test < 0.02 THEN 1 ELSE 0 END                           AS is_test,
           -- 未付 15% / 已付 78% / 部分退 4% / 全退 3%
           CASE WHEN r_pay < 0.15 THEN 0
                WHEN r_pay < 0.93 THEN 1
                WHEN r_pay < 0.97 THEN 2
                ELSE 3 END                                                     AS pay_st
    FROM base
),
calc AS (
    SELECT *,
           amt_item + amt_frght                                                AS amt_total,
           GREATEST(amt_item - amt_disc - amt_cpn + amt_frght, 0)              AS amt_pay
    FROM money
)
SELECT ord_id, ord_ln, buyer_id,
       100000 + (calc.shop_id % 900)::bigint                       AS seller_id,
       calc.shop_id,
       900000 + cat_id * 1000 + (r_misc * 999)::int           AS item_id,
       9000000 + (r_misc * 99999)::bigint                     AS sku_id,
       cat_id,
       1 + cat_id % 8                                         AS cat_id_l1,
       500000 + buyer_id                                      AS addr_id,
       CASE WHEN pay_st > 0 THEN 7000000 + i ELSE NULL END    AS pay_id,
       CASE WHEN r_disc > 0.6 THEN 300 + (r_disc * 40)::int ELSE NULL END AS promo_id,
       CASE WHEN amt_cpn > 0 THEN 800 + (r_cpn * 90)::int ELSE NULL END   AS cpn_id,
       CASE WHEN pay_st > 0 THEN 6000000 + i ELSE NULL END    AS lgst_id,

       amt_total, amt_pay, amt_item, amt_frght, amt_disc, amt_cpn,
       -- 退款只发生在部分退与全退上
       CASE WHEN pay_st = 3 THEN amt_pay
            WHEN pay_st = 2 THEN (amt_pay * (0.2 + r_rfnd * 0.4))::bigint
            ELSE 0 END                                        AS amt_rfnd,
       (amt_pay * 0.06)::bigint                               AS amt_tax,
       (amt_item * (0.5 + r_misc * 0.25))::bigint             AS amt_cost,
       (r_misc * 500)::bigint                                 AS amt_pt,
       NULL::bigint                                           AS price_old,
       qty,
       CASE WHEN pay_st = 3 THEN qty
            WHEN pay_st = 2 THEN GREATEST((qty * 0.5)::int, 1)
            ELSE 0 END                                        AS qty_rfnd,
       (100 + r_misc * 4900)::int                             AS wt_g,

       -- 状态白名单：**有效订单是 2/3/4**，而 1 与 5 从没付过钱、6 已全退
       (CASE WHEN pay_st = 0 THEN (CASE WHEN r_misc < 0.6 THEN 1 ELSE 5 END)
             WHEN pay_st = 1 THEN (CASE WHEN r_misc < 0.2 THEN 2
                                        WHEN r_misc < 0.5 THEN 3 ELSE 4 END)
             WHEN pay_st = 2 THEN 4
             ELSE 6 END)::smallint                            AS ord_st,
       pay_st::smallint,
       (CASE WHEN pay_st = 0 THEN 0 WHEN r_misc < 0.3 THEN 1 ELSE 2 END)::smallint AS lgst_st,
       (CASE WHEN pay_st = 2 THEN 1 WHEN pay_st = 3 THEN 2 ELSE 0 END)::smallint   AS rfnd_st,
       is_test::smallint,
       (CASE WHEN r_misc < 0.03 THEN 1 ELSE 0 END)::smallint  AS is_gift,
       (CASE WHEN r_misc > 0.95 THEN 1 ELSE 0 END)::smallint  AS is_presale,
       (CASE WHEN r_1st < 0.18 THEN 1 ELSE 0 END)::smallint   AS is_1st,
       (CASE WHEN pay_st IN (2, 3) THEN 1 ELSE 0 END)::smallint AS flg_r,
       (1 + floor(r_chnl * 4)::int)::smallint                   AS chnl,
       (1 + floor(r_misc * 4)::int)::smallint                   AS pay_mtd,

       d_crt::timestamptz + ((r_misc * 86400)::int || ' seconds')::interval AS dt_crt,
       CASE WHEN pay_st > 0 THEN d_crt::timestamptz + (((r_misc + 0.1) * 86400)::int || ' seconds')::interval END AS dt_pay,
       CASE WHEN pay_st > 0 AND r_misc > 0.3 THEN d_crt::timestamptz + INTERVAL '1 day' END AS dt_shp,
       CASE WHEN pay_st IN (1, 2) AND r_misc > 0.5 THEN d_crt::timestamptz + INTERVAL '4 days' END AS dt_fin,
       CASE WHEN pay_st IN (2, 3) THEN d_crt::timestamptz + INTERVAL '9 days' END AS dt_rfnd,
       d_crt AS stat_dt,

       s.shop_nm,
       (ARRAY['手机','笔记本','平板','耳机','智能手表','空调','冰箱','洗衣机','面膜','口红',
              '洗发水','牛奶','坚果','咖啡','运动鞋','卫衣','连衣裙','背包','图书','玩具'])[1 + cat_id % 20]
         AS cat_nm,
       (ARRAY['数码','家电','美妆','食品','服饰','运动','图书','母婴'])[1 + cat_id % 8] AS cat_nm_l1,
       (ARRAY['旗舰','轻薄','便携','家用','专业','入门'])[1 + floor(r_misc * 6)::int] || '款商品'
         || (900000 + cat_id * 1000) AS item_nm,
       (ARRAY['华为','小米','苹果','三星','美的','海尔','欧莱雅','雀巢',NULL])[1 + floor(r_misc * 9)::int] AS brand_nm,
       (ARRAY['广东','浙江','江苏','山东','河南','四川','湖北','湖南','河北','福建',
              '安徽','陕西','江西','辽宁','重庆','北京','上海','天津','广西','云南'])[1 + floor(r_chnl * 20)::int] AS prov,
       (ARRAY['深圳','广州','杭州','南京','济南','郑州','成都','武汉','长沙','石家庄',
              '福州','合肥','西安','南昌','沈阳','重庆','北京','上海','天津','南宁'])[1 + floor(r_chnl * 20)::int] AS city,
       (1 + floor(r_1st * 5)::int)::smallint AS buyer_lvl,
       (ARRAY['search','feed','push','ad','direct','share'])[1 + floor(r_chnl * 6)::int] AS src,
       (ARRAY['ios','android','pc','h5'])[1 + floor(r_misc * 4)::int] AS dev,

       1::smallint AS ver,
       CASE WHEN r_misc < 0.05 THEN '客服备注：' || (r_misc * 1000)::int END AS rmk,
       '{"ab":"' || (1 + (r_misc * 3)::int) || '"}' AS ext
FROM calc
JOIN dim_shop s ON s.shop_id = calc.shop_id;

CREATE INDEX ON dwd_ord_dtl (stat_dt);
CREATE INDEX ON dwd_ord_dtl (ord_st, pay_st);
ANALYZE;
