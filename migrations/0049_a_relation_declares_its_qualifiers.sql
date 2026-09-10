-- 谓语带属性（0037）。
--
-- `A invested B` 这条边上的「金额 50 亿」不是第二个宾语——宾语是 B，金额是这条边
-- 自己的属性。从前它没地方放：`object_value` 是属性事实的宾语（`valuation → 5B`
-- 里的 5B），拿它放边上的金额，等于说这条边的宾语既是 B 又是 5B。
--
-- 两张表，都是纯增量，`facts` 一列不动：
--   relation_type_qualifiers  一个关系声明自己能带哪些属性。属性定义复用
--                             `relation_types` 里 kind='attribute' 的行——datatype、
--                             unit、换算都是现成的，只是它的 domain 是一个关系而不是
--                             一个类。同库、且必须是 attribute，这两条在 store 里校验
--                             （CHECK 引不到别的行）。
--   fact_qualifiers           一条事实上挂的属性值。形状与 object_value 一致：
--                             {"value": …, "unit": …}，单位随事实落笔。
--
-- 身份：属性**不进**事实的去重键。同一条边再听到一次带了金额的，是同一条边补上
-- 金额；同一条边两次金额打架，另立一行并记 fact_conflicts，交给人裁——账本里两次
-- 观察不一致从来都是两行 + 一条冲突，这里不例外。

CREATE TABLE relation_type_qualifiers (
    relation_type_id  UUID NOT NULL REFERENCES relation_types(id) ON DELETE CASCADE,
    qualifier_type_id UUID NOT NULL REFERENCES relation_types(id) ON DELETE CASCADE,
    PRIMARY KEY (relation_type_id, qualifier_type_id),
    CHECK (relation_type_id <> qualifier_type_id)
);

CREATE TABLE fact_qualifiers (
    fact_id           UUID  NOT NULL REFERENCES facts(id)          ON DELETE CASCADE,
    qualifier_type_id UUID  NOT NULL REFERENCES relation_types(id) ON DELETE CASCADE,
    -- 二选一：字面值（金额、比例、日期），或指向一个实体（「经 C 撮合」里的 C）。
    -- 这一刀只写字面值；实体那一格是「边能指向节点」这层地位的位置，先留着，
    -- 免得下一次再迁。哪一格有值由 qualifier_type 的 kind 决定（attribute / relation）
    value             JSONB,
    entity_id         UUID REFERENCES entities(id) ON DELETE CASCADE,
    PRIMARY KEY (fact_id, qualifier_type_id),
    CHECK ((value IS NOT NULL) <> (entity_id IS NOT NULL))
);

-- 读边时要把它的属性一起带出来，按事实取
CREATE INDEX fact_qualifiers_fact_idx ON fact_qualifiers (fact_id);
-- 本体页要列「哪些关系带这个属性」，按属性取
CREATE INDEX relation_type_qualifiers_qualifier_idx ON relation_type_qualifiers (qualifier_type_id);
