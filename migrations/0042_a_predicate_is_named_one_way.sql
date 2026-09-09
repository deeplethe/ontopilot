-- 0042：谓词的显示名只有一种写法——**小驼峰**。
--
-- 一个装了 schema.org 的库里，谓词名同时存在四种形态：`acceptedAnswer`（3819）、
-- `owns`（966）、`ApplicableCertificate`（60）、`access to`（88）。前两种是同一种
-- 写法的长短两例，后两种是另外两种写法。在左栏那一列里挨着排，读起来就是没规矩。
--
-- 定的规矩与 RDF/OWL、schema.org 一致，**而且它带信息量**：
--
--   类   → 大驼峰   Person / CreativeWork
--   谓词 → 小驼峰   worksFor / acceptedAnswer / accessTo
--
-- 大小写本身就在说这个词是类还是属性,一眼分得开。库里的类已经是大驼峰
-- （2774 条,另 3 条是 schema.org 自己的 `3DModel`,数字开头,不动），所以这一刀
-- 只动谓词。
--
-- **改 label 是安全的**：它从来不是身份。4875 条谓词带 `iri`，RDF 导出优先用
-- `iri`、没有才从 `key` 铸一个，任何导出路径都不读 label。`key` 也早就归一成
-- snake_case（4996/4999 与 label 只差大小写和分隔符）。
--
-- **只动写错的那些，不从 key 反推**。反推会把 `productID` 变成 `productId`——
-- to_key 在连续大写之间不插下划线，`product_id` 再拼回去就丢了缩写。所以：
--   含分隔符的 → 拼成小驼峰，每一段除首字母外原样保留
--   首字母大写的 → 只小写第一个字符
-- 两条都不碰词内部的大小写，`productID`、`hasLEI`、`accessibilityAPI` 原样留着。
-- （落这一刀时查过：含分隔符的 88 条里 0 条带连续大写，大写开头的 60 条里
-- 0 条以连续大写开头,所以这两条规则在现有数据上不会伤到任何缩写。）
--
-- 那 60 条大写开头的全部来自 unece.org，且 label 就是 IRI 末段——不是导入弄错了，
-- 是那份词表自己不一致（同一个命名空间里 `brandName` 又是小驼峰）。这里有意
-- 偏离它的局部名，因为 `iri` 把真身留住了。

UPDATE relation_types
SET label = (
    SELECT string_agg(
             CASE WHEN i = 1
                  THEN lower(left(p, 1)) || substr(p, 2)
                  ELSE upper(left(p, 1)) || substr(p, 2)
             END, '' ORDER BY i)
    FROM unnest(regexp_split_to_array(btrim(label), '[ _-]+'))
         WITH ORDINALITY AS t(p, i)
    WHERE p <> ''
)
WHERE label ~ '[ _-]';

UPDATE relation_types
SET label = lower(left(label, 1)) || substr(label, 2)
WHERE label ~ '^[A-Z]';
