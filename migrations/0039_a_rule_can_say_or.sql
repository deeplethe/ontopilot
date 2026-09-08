-- 0039 · 一条规则可以说「或」（#476，决定记录 0029）
--
-- 编号让开 0038：另一条分支已经占了那个号（「a decision records why」），
-- 而共享的开发库里它已经落过库了。
--
-- 从前一条规则的条件是一串合取：全都成立才算数。真实的判据常常是
-- 「这样，或者那样」——今天只能写成两条同名规则推出同一个类，而本体里
-- 没有任何地方说得出它们是同一个判据。
--
-- 加一列 `group_seq`：同组的条件用「与」连，组与组之间用「或」连。
-- 一层，不做任意嵌套——**因为求值器的形状就是一组「与」**：一条命中带着
-- 让它成立的那些前提读数与它们交出来的区间（`derived_facts` / `validity()`），
-- 而「一个析取的前提是哪几条」没有好答案。一组「与」跑一遍今天的算法，
-- 每组各出各的命中，前提、区间、前提一没就退休，全部原样保住。
--
-- **老数据全归第 0 组**：默认值就是它，没有数据迁移要判断，既有规则的
-- 语义一个字不变。
ALTER TABLE attribute_rule_conditions
    ADD COLUMN group_seq INT NOT NULL DEFAULT 0;

-- 同一条规则里，(组, 序) 才是一条条件的位置。原来的 (rule_id, seq) 唯一约束
-- 会挡住两组各自从 0 编号
ALTER TABLE attribute_rule_conditions
    DROP CONSTRAINT attribute_rule_conditions_rule_id_seq_key;
ALTER TABLE attribute_rule_conditions
    ADD CONSTRAINT attribute_rule_conditions_rule_group_seq_key
    UNIQUE (rule_id, group_seq, seq);

-- `is not one of`：对一条**存在的**事实做值判断，所以照样有前提、有区间，
-- 与别的条件同构。而「压根没有这个属性」不在这里——缺失没有前提事实可挂
-- 区间，开放世界里「没记」也不等于「没有」（见 0029 末节）
ALTER TABLE attribute_rule_conditions
    DROP CONSTRAINT attribute_rule_conditions_op_check;
ALTER TABLE attribute_rule_conditions
    ADD CONSTRAINT attribute_rule_conditions_op_check
    CHECK (op IN ('gt', 'gte', 'lt', 'lte', 'between', 'in', 'not_in', 'present'));
