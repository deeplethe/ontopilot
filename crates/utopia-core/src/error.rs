#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("Not found")]
    NotFound,
    #[error("Not signed in or invalid credentials")]
    Unauthorized,
    #[error("You don't have permission to do that")]
    Forbidden,
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Validation(String),
    /// 带稳定 code 的校验错误。**message 仍是英文原句**——它是给不做本地化的
    /// 客户端（MCP、CLI）与日志用的；界面拿 code 去 i18n 里查措辞。
    ///
    /// 界面语言在客户端之后，后端不再拥有 locale（见 docs/decisions/0004），
    /// 所以留在这里的字符串是永久英文。用户能撞到的都该带上 code。
    #[error("{message}")]
    Invalid {
        code: &'static str,
        message: String,
        /// 机器给的补充（cron 解析器的报错之类）。措辞归界面，细节归这里
        detail: Option<String>,
    },
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl AppError {
    pub fn invalid(code: &'static str, message: impl Into<String>) -> Self {
        AppError::Invalid {
            code,
            message: message.into(),
            detail: None,
        }
    }
    pub fn invalid_detail(
        code: &'static str,
        message: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        AppError::Invalid {
            code,
            message: message.into(),
            detail: Some(detail.into()),
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;

/// 标在一个**不会因为重试而变好**的失败上（见 issue #195）。
///
/// 队列的默认假设是「再等一会儿也许就好了」，多数失败确实如此：端点抖一下、
/// 数据库忙一瞬、限流一分钟就过去。余额耗尽不是——三次重试隔着 30 秒、2 分钟、
/// 4 分半，七分钟里没有人会去充值，重试只是把同一句错误重说三遍，而运维需要
/// 看见的那条「失败」被推迟了七分钟才出现。
///
/// **判据留在处理器那一侧，不在队列里。** 什么算没救跟领域有关——
/// `utopia-store` 看不见 `utopia-llm` 的错误类型，也不该看见。处理器把这个标记
/// 挂上去（`err.context(Terminal)`），队列只问「挂了没有」。
///
/// 挂上它不影响别的：告警照报（`observe_job_failure` 一并认这个标记，
/// 否则失败得更快反而没人被告知），`last_error` 照写。
#[derive(Debug, Clone, Copy)]
pub struct Terminal;

impl std::fmt::Display for Terminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("will not recover by retrying")
    }
}

impl std::error::Error for Terminal {}

/// 这次失败被标成不必重试了吗。**只看顶层 Error**——任何挂在 `&str` context 之下的
/// marker 都丢失了类型身份（`Context` 的 `Display` 实现覆盖了 `Error` 实现）。
/// 真实用法见 `utopia-server/src/rss_full_content.rs:137`（`Error::new(Terminal).context(...)`）
/// 与 `main.rs:482`（`anyhow::Error::from(e).context(Terminal)`）——两种都让 marker
/// 留在最外层。任何中间包了 `&str` 上下文的写法（`err.context(Terminal).context("...")`）
/// 在这条链路上是错的，调用方应该改成 `anyhow::Error::new(Terminal).context("...")`。
pub fn is_terminal(err: &anyhow::Error) -> bool {
    err.is::<Terminal>()
}

/// 这次失败不是错了，是**等一下再试**——把任务挂回 `queued`，不烧预算（#526）。
///
/// 跟 [`Terminal`] 是一对：一个是「不会变好，别重试了」，一个是「现在做不了，
/// 等一会儿再做」。两者都是领域判断——抽取器知道本体向量还没补齐，队列看不见
/// 那张图。处理器挂标记（`err.context(Deferred::new(Duration::from_secs(30)))`），
/// `mark_failed` 认这个标记并把 `run_at` 推到未来、把 `attempts` 退回去。
///
/// 跟默认退避的区别：默认的 `30s × attempts²` 是错的——它把「再试一次值得」
/// 的失败按次数指数延后，而 `Deferred` 的语义是「这个时间点过了再来」，
/// 与失败次数无关。两次都因为同一个等待挂回队列，下次 `run_at` 都是同一个
/// 偏移，不会有 60s、120s 的递增。
///
/// **不能与 `Terminal` 同挂**：语义互斥。后挂的胜出，详见 [`is_deferred`] 沿
/// context 链的实现选择——返回最近一个标记，最近挂上去的那一个赢。
#[derive(Debug, Clone, Copy)]
pub struct Deferred {
    pub retry_in: std::time::Duration,
}

impl Deferred {
    pub fn new(retry_in: std::time::Duration) -> Self {
        Self { retry_in }
    }
}

impl std::fmt::Display for Deferred {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 落进 `last_error` 的话要让人能直接读懂，所以写明秒数
        write!(f, "deferred; retry in {:?}", self.retry_in)
    }
}

impl std::error::Error for Deferred {}

/// 最近挂上去的 `Deferred` 标记的等待时长。看顶层的 marker（参见 [`is_terminal`]
/// 的注释——同一个 anyhow 类型约束）。`mark_failed` 里先问 [`is_terminal`]，
/// 再问 `is_deferred`——两个看的是同一层，不会有「层层套娃」的二义性。
pub fn is_deferred(err: &anyhow::Error) -> Option<std::time::Duration> {
    err.downcast_ref::<Deferred>().map(|d| d.retry_in)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// 仓库里的真实用法：`anyhow::Error::new(Terminal).context(...)`——
    /// marker 是根，外层 `context` 是给它加的可读说明。链上仍能在 source 找到
    /// `Terminal`（chain 索引 1）。
    ///
    /// 注意 `anyhow::anyhow!("...").context(Terminal)` 是错的写法：内层 `&str`
    /// 触发了 `Context` 的 `Display` 实现而非 `Error` 实现，marker 会变成
    /// display-only context 而 `e.is::<Terminal>()` 找不到它。
    #[test]
    fn terminal_is_recognised_through_context() {
        let err: anyhow::Error = anyhow::Error::new(Terminal).context("balance gone: third retry");
        assert!(is_terminal(&err));
        assert!(is_deferred(&err).is_none());
    }

    #[test]
    fn deferred_is_recognised_through_context() {
        let err: anyhow::Error = anyhow::Error::new(Deferred::new(Duration::from_secs(30)))
            .context("waiting on ontology index");
        assert!(!is_terminal(&err));
        assert_eq!(is_deferred(&err), Some(Duration::from_secs(30)));
    }

    /// 处理器常这样用：把 marker 挂在一个真正的 Error 上（不是字符串）。
    /// 这是 `utopia-server/src/rss_full_content.rs:137` 的写法。marker 必须留在
    /// 顶层，不能再用 `.context(&str)` 包它——见 `is_terminal` 注释里说的
    /// anyhow `Context` `Display`-impl 覆盖问题。
    #[test]
    fn marker_attached_via_context_method_works() {
        let inner: anyhow::Error = anyhow::Error::msg("network blip");
        let marked: anyhow::Error =
            anyhow::Error::new(Deferred::new(Duration::from_secs(7))).context(inner);
        assert_eq!(is_deferred(&marked), Some(Duration::from_secs(7)));

        let inner_t: anyhow::Error = anyhow::Error::msg("balance gone");
        let marked_t: anyhow::Error = anyhow::Error::new(Terminal).context(inner_t);
        assert!(is_terminal(&marked_t));
    }

    /// 两个 marker 都挂在顶层时由调用方决定谁胜出——`is_terminal` 在 `mark_failed`
    /// 里先问，`is_deferred` 后问。这个测试只是把行为钉死，将来谁动了优先级
    /// 都会看到这一处失败。
    #[test]
    fn terminal_takes_priority_at_top_level() {
        let err: anyhow::Error = anyhow::Error::new(Deferred::new(Duration::from_secs(10)));
        let err: anyhow::Error = anyhow::Error::new(Terminal).context(err);
        assert!(is_terminal(&err));
        assert!(is_deferred(&err).is_none());
    }

    /// 一个普通错误不该被认成 `Deferred` 或 `Terminal`
    #[test]
    fn plain_error_is_neither() {
        let err: anyhow::Error = anyhow::Error::msg("network blip");
        assert!(!is_terminal(&err));
        assert!(is_deferred(&err).is_none());
    }
}
