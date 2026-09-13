-- TPC-H 的列注释。**单独一个文件，因为它是一个自变量。**
--
-- 真 TPC-H 一条注释都没有，而真实库里注释是探索最主要的线索——
-- `explore_mappings` 把它原样拼进 prompt（`-- {comment}`），提示词还专门
-- 要求「citing column comments」。加载与不加载各跑一轮，两个分数之差
-- 就是注释值多少分。
--
-- **写法上守一条线：注释描述列，不描述口径。** 写「折后收入 =
-- extendedprice × (1 - discount)」等于把真值的 gold SQL 抄给模型，
-- 量出来的就不是它能不能读懂 schema 了。所以只说这一列是什么、
-- 什么单位、取值域多大——真实的 DBA 也就写到这个份上。
--
-- 几列故意不写：地址、电话、以及 *_comment 那几列里除 l_comment 外的。
-- 真实库不会每列都有注释，而「没注释的列会被怎么处理」本身要量。

SET search_path TO tpch;

COMMENT ON TABLE  region   IS 'Sales regions; five rows, the top of the geography hierarchy';
COMMENT ON COLUMN region.r_name IS 'Region name (AFRICA, AMERICA, ASIA, EUROPE, MIDDLE EAST)';

COMMENT ON TABLE  nation   IS 'Countries, each belonging to one region';
COMMENT ON COLUMN nation.n_name      IS 'Country name, uppercase';
COMMENT ON COLUMN nation.n_regionkey IS 'The region this country belongs to';

COMMENT ON TABLE  supplier IS 'Suppliers of parts';
COMMENT ON COLUMN supplier.s_nationkey IS 'Country the supplier is based in';
COMMENT ON COLUMN supplier.s_acctbal   IS 'Account balance in USD; may be negative';

COMMENT ON TABLE  customer IS 'Customers placing orders';
COMMENT ON COLUMN customer.c_nationkey  IS 'Country the customer is based in';
COMMENT ON COLUMN customer.c_acctbal    IS 'Account balance in USD; may be negative';
COMMENT ON COLUMN customer.c_mktsegment IS 'Market segment: AUTOMOBILE, BUILDING, FURNITURE, MACHINERY or HOUSEHOLD';

COMMENT ON TABLE  part     IS 'Parts catalogue';
COMMENT ON COLUMN part.p_mfgr        IS 'Manufacturer, Manufacturer#1..5';
COMMENT ON COLUMN part.p_brand       IS 'Brand, Brand#11..55; a brand belongs to one manufacturer';
COMMENT ON COLUMN part.p_type        IS 'Material and finish, space separated, e.g. PROMO BRUSHED STEEL';
COMMENT ON COLUMN part.p_size        IS 'Nominal size, 1..50, unitless';
COMMENT ON COLUMN part.p_container   IS 'Packaging, e.g. SM CASE, JUMBO BOX';
COMMENT ON COLUMN part.p_retailprice IS 'List price per unit in USD';

COMMENT ON TABLE  partsupp IS 'Which supplier can supply which part, and on what terms';
COMMENT ON COLUMN partsupp.ps_availqty   IS 'Units this supplier currently holds of this part';
COMMENT ON COLUMN partsupp.ps_supplycost IS 'What we pay this supplier per unit, USD';

COMMENT ON TABLE  orders   IS 'Customer orders; one row per order, line detail in lineitem';
COMMENT ON COLUMN orders.o_custkey       IS 'Customer who placed the order';
COMMENT ON COLUMN orders.o_orderstatus   IS 'O = all lines still open, F = all lines fulfilled, P = partially fulfilled';
COMMENT ON COLUMN orders.o_totalprice    IS 'Order total in USD, taxed and discounted, equal to the sum over its lines';
COMMENT ON COLUMN orders.o_orderdate     IS 'Date the order was placed';
COMMENT ON COLUMN orders.o_orderpriority IS 'Priority, 1-URGENT .. 5-LOW';
COMMENT ON COLUMN orders.o_clerk         IS 'Clerk who took the order';

COMMENT ON TABLE  lineitem IS 'Order lines; the fact table. One row per part on an order';
COMMENT ON COLUMN lineitem.l_orderkey      IS 'The order this line belongs to';
COMMENT ON COLUMN lineitem.l_partkey       IS 'The part sold on this line';
COMMENT ON COLUMN lineitem.l_suppkey       IS 'The supplier that supplied it';
COMMENT ON COLUMN lineitem.l_linenumber    IS 'Position of this line within its order, 1-based';
COMMENT ON COLUMN lineitem.l_quantity      IS 'Units sold on this line';
COMMENT ON COLUMN lineitem.l_extendedprice IS 'Line amount in USD before discount and tax: units × the part list price';
COMMENT ON COLUMN lineitem.l_discount      IS 'Discount granted on this line as a fraction, 0.00 to 0.10';
COMMENT ON COLUMN lineitem.l_tax           IS 'Tax rate applied to this line as a fraction, 0.00 to 0.08';
COMMENT ON COLUMN lineitem.l_returnflag    IS 'R = returned by the customer, A = accepted, N = not yet settled';
COMMENT ON COLUMN lineitem.l_linestatus    IS 'F = fulfilled, O = still open';
COMMENT ON COLUMN lineitem.l_shipdate      IS 'Date the line shipped';
COMMENT ON COLUMN lineitem.l_commitdate    IS 'Date we committed to the customer';
COMMENT ON COLUMN lineitem.l_receiptdate   IS 'Date the customer received it; later than commitdate means late';
COMMENT ON COLUMN lineitem.l_shipmode      IS 'Carrier mode: REG AIR, AIR, RAIL, SHIP, TRUCK, MAIL or FOB';
COMMENT ON COLUMN lineitem.l_shipinstruct  IS 'Handling instruction on the shipment';
COMMENT ON COLUMN lineitem.l_comment       IS 'Free-text remark typed by staff; no analytical meaning';
