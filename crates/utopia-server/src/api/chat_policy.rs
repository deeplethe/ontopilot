//! 聊天循环里的「策略」——每个分支抽成一个具名函数，让调用方一眼看见每条规则，\
//! 而不是埋在 `loop { match { ... } }` 里的一连串分支。
//!
//! **为什么不是 trait+chain（#546）？** 长远方向是把循环搬到 `rig-core 0.42` 上，\
//! 让 `on_completion_call` / `ModelTurnAction` / `ToolChoice` 当第一公民——那是\
//! #546 推荐的路径。但这一步是周级的端口，不属于本次改动。本模块先把每条\
//! 策略提为 `pub(super) fn`，让「策略是什么」从一串 `if` 中独立出来；将来 trait\
//! 化时，把它们各自包成一个 `impl ChatPolicy` 就是翻译，不是重写。
//!
//! 现有五条策略（来自 #509、#543、#546 的实测十五轮）：
//!
//! 1. [`no_tool_on_empty_answer`]——一轮没调工具又没正文，错误而非空答案。
//! 2. [`force_final_answer_message`]——预算用尽，命令模型作答。
//! 3. [`should_nudge_stalled_turn`]——只问一次：这一轮停住了吗？（#509）
//! 4. [`should_degrade_to_one_shot_rag`]——首轮错就退化到一次性 RAG。
//! 5. [`reject_invalid_tool_call`]——参数没说清的调用不执行（`check_call` 的同伙）。
//!
//! 每条都返回**决定**与**为什么**——SSE 协议（`step*|sources|delta*→done|error`）\
//! 由调用方自己发，这里不直接 yield 帧。理由：抽帧意味着策略耦合了流的形状，\
//! 那 trait 化时就拆不开——而 trait 化是下一步。
//!
//! 这五条的判定逻辑都是从 `chat.rs:790..` 那一段循环搬过来的——行为字节级不变，\
//! 只是搬家。`chat.rs` 里的代码改成调用这些函数。

use serde_json::{json, Value};
use utopia_llm::AssistantTurn;

/// 一轮没调工具又没正文——返回 `Some(reason)` 让调用方发错误帧。
/// 否则 `None`，循环继续走「停住了吗？」那条路。
pub(super) fn no_tool_on_empty_answer(
    answer_acc: &str,
    tool_calls: &[utopia_llm::ToolCall],
) -> Option<&'static str> {
    if tool_calls.is_empty() && answer_acc.is_empty() {
        Some("Model returned an empty answer")
    } else {
        None
    }
}

/// 工具预算用尽，命令模型就现有证据作答。**这只是一句命令文本**——SSE 的事
/// 仍由调用方做（`stream_raw` + `done`），策略本身不碰流。
pub(super) fn force_final_answer_message() -> Value {
    json!({
        "role": "user",
        "content": "(system) Tool budget exhausted. Answer now from the evidence gathered above.",
    })
}

/// 一轮「没调工具也没正文以外的内容」之后，是否追问一次。
///
/// `#509` 的追问只给一次、不流式、只认三种回复：
/// - 调工具 → 照常执行（`AfterNudge::Tools`）
/// - 说 DONE / 沉默 → 原样收尾（`AfterNudge::Done`）
/// - 又是一段文字 → 还在说空话（`AfterNudge::Stalled`）
///
/// `nudged` 是这一轮的「已经问过吗」标记。本函数返回 `true` 当且仅当：
///   - 这一轮没调工具
///   - 轨迹与引用都为空
///   - 这一轮还没追过
pub(super) fn should_nudge_stalled_turn(
    nudged: bool,
    steps: &[Value],
    sources: &[Value],
    tool_calls: &[utopia_llm::ToolCall],
) -> bool {
    !nudged && tool_calls.is_empty() && answer_rests_on_nothing(steps, sources)
}

/// 一轮结束、正文非空、整场没调过工具也没有引用：这个答案什么都不站在上面。
/// 「问这场对话」的消息也满足这两条，所以追问必须便宜、安静，且留有 DONE 出口。
pub(super) fn answer_rests_on_nothing(steps: &[Value], sources: &[Value]) -> bool {
    steps.is_empty() && sources.is_empty()
}

/// 追问之后模型的回复算哪种。提到模块里是因为 `should_nudge_stalled_turn` 决定
/// 要不要问，而本枚举决定问完之后怎么办——两条同源，留一起好读。
#[derive(Debug, PartialEq, Eq)]
pub(super) enum AfterNudge {
    /// 调了工具：照常执行，接着走
    Tools,
    /// 说上一句已经是答案，或者什么都没说：原样收尾
    Done,
    /// 又是一段文字：还在说空话
    Stalled,
}

pub(super) fn classify_nudge_response(turn: &AssistantTurn) -> AfterNudge {
    if !turn.tool_calls.is_empty() {
        return AfterNudge::Tools;
    }
    let said = turn
        .content
        .as_deref()
        .map(|t| t.trim().trim_matches(|c: char| !c.is_alphanumeric()))
        .unwrap_or_default();
    if said.is_empty() || said.eq_ignore_ascii_case("done") {
        AfterNudge::Done
    } else {
        AfterNudge::Stalled
    }
}

/// 首轮（rounds == 0）调工具的流式出错就退化到一次性 RAG。**别用在后续轮次**——
/// 后续轮次出错是真出错，不该被当作「这个模型没工具能力」而吞掉（实测里
/// SiliconFlow 的网络抖动触发过这条不该走的分支）。
pub(super) fn should_degrade_to_one_shot_rag(rounds: usize, has_error: bool) -> bool {
    rounds == 0 && has_error
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_calls() -> Vec<utopia_llm::ToolCall> {
        Vec::new()
    }

    #[test]
    fn no_tool_on_empty_answer_fires_on_empty() {
        assert_eq!(
            no_tool_on_empty_answer("", &no_calls()),
            Some("Model returned an empty answer")
        );
    }

    #[test]
    fn no_tool_on_empty_answer_silent_when_text_present() {
        // 正文非空就不报错——它可能正好是「我查了再说」之类的承诺，
        // 那是 #509 的事，不是这条规则的事
        assert_eq!(
            no_tool_on_empty_answer("好的，让我查一下。", &no_calls()),
            None
        );
    }

    #[test]
    fn force_final_answer_message_has_a_user_role() {
        let m = force_final_answer_message();
        assert_eq!(m["role"], "user");
        assert!(m["content"]
            .as_str()
            .unwrap()
            .contains("Tool budget exhausted"));
    }

    #[test]
    fn nudge_only_fires_once() {
        let steps: Vec<Value> = Vec::new();
        let sources: Vec<Value> = Vec::new();
        // 第一次：可以问
        assert!(should_nudge_stalled_turn(
            false,
            &steps,
            &sources,
            &no_calls()
        ));
        // 第二次：`nudged=true`，不让再问
        assert!(!should_nudge_stalled_turn(
            true,
            &steps,
            &sources,
            &no_calls()
        ));
    }

    #[test]
    fn nudge_does_not_fire_when_tools_were_called() {
        let steps: Vec<Value> = Vec::new();
        let sources: Vec<Value> = Vec::new();
        // 调了工具就不算停住——让循环正常收这一轮的交换
        let calls = vec![utopia_llm::ToolCall {
            id: "x".into(),
            name: "search_chunks".into(),
            arguments: "{}".into(),
        }];
        assert!(!should_nudge_stalled_turn(false, &steps, &sources, &calls));
    }

    #[test]
    fn nudge_does_not_fire_when_sources_are_present() {
        // 有引用就说明之前查到过东西——再问一次不会得到「现在我去查」的回答，
        // 直接收尾
        let steps: Vec<Value> = Vec::new();
        let sources: Vec<Value> = vec![json!({ "n": 1 })];
        assert!(!should_nudge_stalled_turn(
            false,
            &steps,
            &sources,
            &no_calls()
        ));
    }

    #[test]
    fn nudge_classifies_tools_done_and_stalled() {
        let tools = AssistantTurn {
            content: None,
            tool_calls: vec![utopia_llm::ToolCall {
                id: "x".into(),
                name: "search_chunks".into(),
                arguments: "{}".into(),
            }],
        };
        assert_eq!(classify_nudge_response(&tools), AfterNudge::Tools);

        let done = AssistantTurn {
            content: Some("done".into()),
            tool_calls: vec![],
        };
        assert_eq!(classify_nudge_response(&done), AfterNudge::Done);

        let silent = AssistantTurn {
            content: Some("".into()),
            tool_calls: vec![],
        };
        assert_eq!(classify_nudge_response(&silent), AfterNudge::Done);

        let stalled = AssistantTurn {
            content: Some("请稍等，我去搜一下".into()),
            tool_calls: vec![],
        };
        assert_eq!(classify_nudge_response(&stalled), AfterNudge::Stalled);
    }

    #[test]
    fn degradation_only_on_round_zero() {
        assert!(should_degrade_to_one_shot_rag(0, true));
        // 后续轮次出错是真出错，不能退路——把整个失败隐藏掉
        assert!(!should_degrade_to_one_shot_rag(1, true));
        assert!(!should_degrade_to_one_shot_rag(0, false));
    }
}
