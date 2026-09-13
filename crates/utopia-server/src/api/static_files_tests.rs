//! 构建产物那一段的契约（#614 / #616）。不连库：`with_static_files` 不带 state。
//!
//! 这段代码破过两次，两次都是同一个形状——**一条路的失败被另一条路的成功语义
//! 盖住了**。第一次是缺文件回首页（浏览器按模块脚本解析一张网页，白屏）；
//! 第二次是缺文件的 404 也被扣上 `immutable`，于是「这个文件不存在」被浏览器
//! 记一年，而资源名带哈希，下一次部署请求的正是那个刚被记下的名字。
//!
//! 所以这里逐条钉的是**状态码、Content-Type 与缓存指示三者一起**：少看一样，
//! 上面两次都能溜过去。
use super::with_static_files;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use tower::ServiceExt;

/// 一个最小的 dist：一张首页，一个带哈希名的产物。
fn dist() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(
        dir.path().join("index.html"),
        "<!doctype html><title>u</title>",
    )
    .expect("index");
    std::fs::create_dir(dir.path().join("assets")).expect("assets dir");
    std::fs::write(
        dir.path().join("assets/app-d34db33f.js"),
        "export default 1;",
    )
    .expect("asset");
    dir
}

async fn get(path: &str) -> (StatusCode, Option<String>, Option<String>) {
    let dir = dist();
    let app = with_static_files(Router::new(), dir.path().to_str().expect("utf-8 path"));
    let res = app
        .oneshot(
            Request::builder()
                .uri(path)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let head = |name: &str| {
        res.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    (res.status(), head("content-type"), head("cache-control"))
}

#[tokio::test]
async fn a_hashed_asset_is_served_and_may_be_kept_for_a_year() {
    let (status, ctype, cache) = get("/assets/app-d34db33f.js").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ctype.as_deref(), Some("text/javascript"));
    assert_eq!(
        cache.as_deref(),
        Some("public, max-age=31536000, immutable")
    );
}

/// **缺掉的产物是 404，而且那条 404 不许被存起来。**
/// 回首页会白屏（#616）；把 404 存一年，则下一次部署的新哈希在那台机器上
/// 一年拿不到——两条都要守，所以两个断言都在这里。
#[tokio::test]
async fn a_missing_asset_is_a_404_that_nobody_keeps() {
    let (status, ctype, cache) = get("/assets/app-00000000.js").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_ne!(ctype.as_deref(), Some("text/html"), "404 不许是一张网页");
    assert_eq!(cache.as_deref(), Some("no-cache"));
}

/// 页面路由刷新拿首页——history fallback 还在，这一条是上面那条的对照。
#[tokio::test]
async fn a_page_route_still_gets_the_index() {
    for path in ["/", "/graph", "/kb/anything/deep"] {
        let (status, ctype, cache) = get(path).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert_eq!(ctype.as_deref(), Some("text/html"), "{path}");
        assert_eq!(
            cache.as_deref(),
            Some("no-cache"),
            "{path} 的首页必须回源确认"
        );
    }
}
