-- 新建的库，四个自动化开关一律缺省开。
--
-- 从前是两开两关：自动扩本体、抽取后消解类型缺省开，物化推理与治理缺省关。
-- 关着的那两个各有当时的理由，两条都过期了。
--
-- `materialize_inferences`（0002 决定 6）关着，理由是「声明可能是错的，不该在
-- 人没表态时就按它改图」。可派生事实从来带标记、单列一段、随时撤得掉——它加的是
-- 一层能整片摘掉的东西，碰不到账本里人写的那部分。而关着的代价是新库的图一直缺
-- 传递链和对称对：人得先知道有这么个开关，才看得见本来就该看见的边。
--
-- `governance`（0025 决定 2）关着，理由是「合并实体这一档还没在任何库上量过，
-- 一个库要先有历史可读才配自动」。0025 决定 4 改写之后量过了：标注库上自裁 97.7%、
-- 同意 96.7%、错并 6 对（其中 4 对是模型来回翻的同一对），留给人 12 对（#458）。
-- 闸门把过不去的一律写成建议，每一笔都列在审核页、都可撤。
--
-- 只改缺省，不动已经建好的库：那些值是各自库主表过的态。
-- `governance_since` 一并给缺省——库生下来就开着治理，那个「打开治理的时刻」就是
-- 建库的时刻；留成 NULL 的话保险丝（0025 决定 9）只好从整个窗口起算。

ALTER TABLE knowledge_bases ALTER COLUMN materialize_inferences SET DEFAULT TRUE;
ALTER TABLE knowledge_bases ALTER COLUMN governance             SET DEFAULT TRUE;
ALTER TABLE knowledge_bases ALTER COLUMN governance_since       SET DEFAULT now();
