#!/bin/bash
# 创建应用运行时使用的受限角色。仅在数据目录为空的首次初始化时执行
# （Postgres 官方镜像的 docker-entrypoint-initdb.d 约定），既有部署不受影响。
#
# 权限不在这里给——由迁移 0031 授予，那样每次升级都能把新表补进来。
# 这里只负责角色本身存在，且拥有一个可登录的口令。
#
# **这个文件可能被 source，不是被执行。** Postgres 的 entrypoint 对没有执行位的
# `*.sh` 用 `. "$f"` 读进它自己的 shell（Linux/macOS 上 clone 下来的文件就没有执行位；
# Windows 的 Docker Desktop 把 bind mount 全显示成可执行，所以本机看不出来）。
# 被 source 的脚本里一句 `exit 0` 退的是 entrypoint——数据库容器在 initdb 之后、
# 真正起 postgres 之前以 0 退出，app 跟着报 "dependency db failed to start"（#456）。
# 所以这里不写 exit，也不动 shell 选项（`set -u` 同样会改到 entrypoint）。
set -e

if [ -z "${UTOPIA_APP_DB_PASSWORD:-}" ]; then
    echo "未设置 UTOPIA_APP_DB_PASSWORD，跳过受限角色创建；应用将以 owner 身份运行。" >&2
else
    psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<SQL
DO \$\$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'utopia_app') THEN
        CREATE ROLE utopia_app LOGIN PASSWORD '${UTOPIA_APP_DB_PASSWORD}';
        RAISE NOTICE '已创建受限角色 utopia_app';
    END IF;
END
\$\$;
GRANT CONNECT ON DATABASE "$POSTGRES_DB" TO utopia_app;
SQL
fi
