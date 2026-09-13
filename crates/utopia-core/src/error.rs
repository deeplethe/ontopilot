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

/// 这次失败被标成不必重试了吗。
///
/// **问 `anyhow::Error::is`，不沿 `chain()` 逐个问。** 标记挂成 context
/// （`e.context(Terminal)`，`main.rs` 两处都这么写）时，链上那一环的具体类型是
/// anyhow 内部的 `ContextError<Terminal, _>`，`dyn Error::is::<Terminal>()` 认不出它，
/// 于是「没救的失败不再退避」从来没有生效。`anyhow::Error::is` 会同时看 context
/// 与被包的错误，并穿过上层再套的 `context(...)`，两种挂法都认得
pub fn is_terminal(err: &anyhow::Error) -> bool {
    err.is::<Terminal>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;

    #[test]
    fn terminal_as_the_context() {
        let err = anyhow::anyhow!("model endpoint gone").context(Terminal);
        assert!(is_terminal(&err));
    }

    #[test]
    fn terminal_as_the_root() {
        let err = anyhow::Error::new(Terminal).context("source_mismatch");
        assert!(is_terminal(&err));
    }

    #[test]
    fn terminal_under_more_context() {
        let err = anyhow::anyhow!("balance gone")
            .context(Terminal)
            .context("job 42");
        assert!(is_terminal(&err));
        let io: Result<(), _> = Err(std::io::Error::other("io"));
        assert!(is_terminal(&io.context(Terminal).unwrap_err()));
    }

    #[test]
    fn a_plain_failure_is_not_terminal() {
        let err = anyhow::anyhow!("network blip").context("job 42");
        assert!(!is_terminal(&err));
    }
}
