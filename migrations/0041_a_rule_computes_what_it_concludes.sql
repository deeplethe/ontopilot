-- 0032：一条规则的结论可以是**算出来的**，不只是写死的一个值。
--
-- 从前 `conclude_value` 是一个 JSONB 字面量，所以写得出 `grade = A`，写不出
-- `margin = revenue − cost`——那个数只好在别处算完再当断言录回来，既丢了它
-- 站着的那两条读数，又在任一条读数变了之后立刻过期。
--
-- 结论多一支 `computed`：谓词照旧在 `conclude_predicate_id`，怎么算在
-- `conclude_expr` 里，存的是一棵树而不是一个字符串：
--
--   {"attr": "<uuid>"} | {"const": 12.5} | {"op": "sub", "l": {…}, "r": {…}}
--
-- **树而不是字符串**，是因为界面照着它渲染、求值照着它算，两者不会漂移；
-- 存字符串再现解析，就等于把「跑的是什么」和「显示的是什么」分成两件事
-- （0002 拒掉用户自定义规则语言时，真正在守的是这一条）。
ALTER TABLE attribute_rules ADD COLUMN conclude_expr JSONB;

-- 三支各填各的那几列，填串了直接挡住。老的两支一个字没动
ALTER TABLE attribute_rules DROP CONSTRAINT attribute_rule_conclusion_shape;
ALTER TABLE attribute_rules ADD CONSTRAINT attribute_rule_conclusion_shape CHECK (
    (conclusion = 'typing'
         AND conclude_type_id IS NOT NULL
         AND conclude_predicate_id IS NULL
         AND conclude_value IS NULL
         AND conclude_expr IS NULL)
    OR
    (conclusion = 'attribute'
         AND conclude_type_id IS NULL
         AND conclude_predicate_id IS NOT NULL
         AND conclude_value IS NOT NULL
         AND conclude_expr IS NULL)
    OR
    (conclusion = 'computed'
         AND conclude_type_id IS NULL
         AND conclude_predicate_id IS NOT NULL
         AND conclude_value IS NULL
         AND conclude_expr IS NOT NULL)
);
ALTER TABLE attribute_rules DROP CONSTRAINT IF EXISTS attribute_rules_conclusion_check;
ALTER TABLE attribute_rules
    ADD CONSTRAINT attribute_rules_conclusion_check
    CHECK (conclusion IN ('typing', 'attribute', 'computed'));

-- 条件那边**一列都不用加**：`operand` 本来就是 JSONB，一个数是数、一个区间是
-- 两元数组、一个集合是字符串数组，而算式是个对象——四种形状互不相同，读的时候
-- 认得出来。那条 CHECK 只管「present 不带操作数、别的都得带」，照旧成立。
