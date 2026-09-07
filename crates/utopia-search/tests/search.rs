use utopia_search::{rrf_fuse, DocsIndex, DocsSection, SearchIndex};

fn open_index(dir: &std::path::Path) -> SearchIndex {
    SearchIndex::open(dir).expect("SearchIndex::open failed")
}

// ---------------------------------------------------------------------------
// SearchIndex — basic indexing and retrieval
// ---------------------------------------------------------------------------

#[test]
fn empty_index_returns_no_results() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = open_index(tmp.path());
    let hits = idx.search("kb1", "anything", 10).unwrap();
    assert!(hits.is_empty());
}

#[test]
fn empty_index_len_is_zero() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = open_index(tmp.path());
    assert_eq!(idx.len(), 0);
    assert!(idx.is_empty());
}

#[test]
fn indexed_document_is_found_by_keyword() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = open_index(tmp.path());

    idx.reindex_document(
        "kb1",
        "doc1",
        &[("chunk1".into(), "knowledge graph extraction".into())],
    )
    .unwrap();

    let hits = idx.search("kb1", "knowledge", 10).unwrap();
    assert!(!hits.is_empty(), "expected at least one hit");
    assert!(
        hits.iter().any(|h| h.chunk_id == "chunk1"),
        "chunk1 not in results: {:?}",
        hits
    );
}

#[test]
fn search_is_scoped_to_kb_id() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = open_index(tmp.path());

    idx.reindex_document(
        "kb_a",
        "doc1",
        &[("chunk_a".into(), "knowledge graph".into())],
    )
    .unwrap();
    idx.reindex_document(
        "kb_b",
        "doc2",
        &[("chunk_b".into(), "knowledge graph".into())],
    )
    .unwrap();

    let hits_a = idx.search("kb_a", "knowledge", 10).unwrap();
    assert!(hits_a.iter().any(|h| h.chunk_id == "chunk_a"));
    assert!(
        !hits_a.iter().any(|h| h.chunk_id == "chunk_b"),
        "kb_b chunk must not appear when searching kb_a"
    );
}

#[test]
fn reindex_document_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = open_index(tmp.path());

    let chunks = vec![("chunk1".into(), "idempotent indexing test".into())];
    idx.reindex_document("kb1", "doc1", &chunks).unwrap();
    idx.reindex_document("kb1", "doc1", &chunks).unwrap(); // second call must not duplicate

    let hits = idx.search("kb1", "idempotent", 10).unwrap();
    assert_eq!(
        hits.len(),
        1,
        "reindexing same document twice must not create duplicate hits"
    );
}

#[test]
fn delete_document_removes_it_from_results() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = open_index(tmp.path());

    idx.reindex_document(
        "kb1",
        "doc1",
        &[("chunk1".into(), "deletable content".into())],
    )
    .unwrap();

    // Confirm it's there before deleting.
    let before = idx.search("kb1", "deletable", 10).unwrap();
    assert!(!before.is_empty(), "document should be found before deletion");

    idx.delete_document("doc1").unwrap();

    let after = idx.search("kb1", "deletable", 10).unwrap();
    assert!(
        after.is_empty(),
        "document should not be found after deletion"
    );
}

#[test]
fn multiple_chunks_all_returned_within_limit() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = open_index(tmp.path());

    let chunks: Vec<(String, String)> = (0..5)
        .map(|i| (format!("chunk{i}"), format!("entity extraction chunk {i}")))
        .collect();
    idx.reindex_document("kb1", "doc1", &chunks).unwrap();

    let hits = idx.search("kb1", "entity extraction", 10).unwrap();
    assert_eq!(hits.len(), 5, "all 5 chunks should be returned");
}

#[test]
fn limit_caps_number_of_results() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = open_index(tmp.path());

    let chunks: Vec<(String, String)> = (0..10)
        .map(|i| (format!("chunk{i}"), format!("graph reasoning document {i}")))
        .collect();
    idx.reindex_document("kb1", "doc1", &chunks).unwrap();

    let hits = idx.search("kb1", "graph reasoning", 3).unwrap();
    assert!(hits.len() <= 3, "limit=3 must cap results at 3");
}

#[test]
fn query_with_only_punctuation_returns_no_results() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = open_index(tmp.path());

    idx.reindex_document("kb1", "doc1", &[("c1".into(), "some content".into())])
        .unwrap();

    // A query that tokenises to nothing should return an empty list, not an error.
    let hits = idx.search("kb1", "... --- ...", 10).unwrap();
    assert!(hits.is_empty());
}

#[test]
fn hits_have_positive_scores() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = open_index(tmp.path());

    idx.reindex_document(
        "kb1",
        "doc1",
        &[("chunk1".into(), "knowledge graph reasoning".into())],
    )
    .unwrap();

    let hits = idx.search("kb1", "knowledge", 10).unwrap();
    for h in &hits {
        assert!(h.score > 0.0, "BM25 score should be positive, got {}", h.score);
    }
}

#[test]
fn len_reflects_chunk_count() {
    let tmp = tempfile::tempdir().unwrap();
    let idx = open_index(tmp.path());

    let chunks: Vec<(String, String)> = (0..3)
        .map(|i| (format!("c{i}"), format!("text {i}")))
        .collect();
    idx.reindex_document("kb1", "doc1", &chunks).unwrap();
    assert_eq!(idx.len(), 3);
    assert!(!idx.is_empty());
}

// ---------------------------------------------------------------------------
// DocsIndex
// ---------------------------------------------------------------------------

#[test]
fn docs_index_empty_sections_returns_no_results() {
    let idx = DocsIndex::build(&[]).unwrap();
    let results = idx.search("anything", 10).unwrap();
    assert!(results.is_empty());
}

#[test]
fn docs_index_finds_section_by_keyword() {
    let sections = vec![DocsSection {
        slug: "getting-started".into(),
        title: "Getting Started".into(),
        heading: "Installation".into(),
        anchor: "installation".into(),
        body: "Install the package with cargo add utopia.".into(),
    }];
    let idx = DocsIndex::build(&sections).unwrap();
    let results = idx.search("cargo", 5).unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0].slug, "getting-started");
    assert_eq!(results[0].heading, "Installation");
}

#[test]
fn docs_index_query_returns_correct_fields() {
    let sections = vec![DocsSection {
        slug: "api-reference".into(),
        title: "API Reference".into(),
        heading: "Authentication".into(),
        anchor: "authentication".into(),
        body: "Use a bearer token to authenticate requests.".into(),
    }];
    let idx = DocsIndex::build(&sections).unwrap();
    let results = idx.search("bearer token", 5).unwrap();
    assert!(!results.is_empty());
    let r = &results[0];
    assert_eq!(r.slug, "api-reference");
    assert_eq!(r.title, "API Reference");
    assert_eq!(r.anchor, "authentication");
    assert!(!r.body.is_empty());
}

#[test]
fn docs_index_unmatched_query_returns_empty() {
    let sections = vec![DocsSection {
        slug: "intro".into(),
        title: "Introduction".into(),
        heading: "Overview".into(),
        anchor: "overview".into(),
        body: "This is the overview section.".into(),
    }];
    let idx = DocsIndex::build(&sections).unwrap();
    let results = idx.search("xyzzy_nonexistent_term_999", 5).unwrap();
    assert!(results.is_empty());
}

// ---------------------------------------------------------------------------
// rrf_fuse
// ---------------------------------------------------------------------------

#[test]
fn rrf_fuse_single_list_preserves_order() {
    let lists = vec![vec!["a".to_string(), "b".to_string(), "c".to_string()]];
    let fused = rrf_fuse(&lists, 10);
    // Original order by RRF rank: a (rank 0) > b (rank 1) > c (rank 2)
    assert_eq!(fused, vec!["a", "b", "c"]);
}

#[test]
fn rrf_fuse_empty_lists_returns_empty() {
    let fused = rrf_fuse(&[], 10);
    assert!(fused.is_empty());

    let fused2 = rrf_fuse(&[vec![]], 10);
    assert!(fused2.is_empty());
}

#[test]
fn rrf_fuse_limit_is_respected() {
    let list: Vec<String> = (0..20).map(|i| i.to_string()).collect();
    let fused = rrf_fuse(&[list], 5);
    assert_eq!(fused.len(), 5);
}

#[test]
fn rrf_fuse_item_in_both_lists_ranks_above_item_in_one() {
    // "shared" appears at rank 0 in both lists; "only_a" and "only_b" appear once each.
    let list_a = vec!["shared".to_string(), "only_a".to_string()];
    let list_b = vec!["shared".to_string(), "only_b".to_string()];
    let fused = rrf_fuse(&[list_a, list_b], 10);

    let pos_shared = fused.iter().position(|x| x == "shared").unwrap();
    let pos_a = fused.iter().position(|x| x == "only_a").unwrap();
    let pos_b = fused.iter().position(|x| x == "only_b").unwrap();

    assert!(
        pos_shared < pos_a && pos_shared < pos_b,
        "item present in both lists should rank first; got: {:?}",
        fused
    );
}

#[test]
fn rrf_fuse_known_scores() {
    // k=60 (the constant in the implementation).
    // List A: ["x", "y"] → x gets 1/(60+1), y gets 1/(60+2)
    // List B: ["y", "x"] → y gets 1/(60+1), x gets 1/(60+2)
    // Total: x = 1/61 + 1/62, y = 1/62 + 1/61 → both equal; order is unspecified but both present.
    let list_a = vec!["x".to_string(), "y".to_string()];
    let list_b = vec!["y".to_string(), "x".to_string()];
    let fused = rrf_fuse(&[list_a, list_b], 10);
    assert_eq!(fused.len(), 2);
    assert!(fused.contains(&"x".to_string()));
    assert!(fused.contains(&"y".to_string()));
}

#[test]
fn rrf_fuse_deduplicates_across_lists() {
    let list_a = vec!["a".to_string(), "b".to_string()];
    let list_b = vec!["a".to_string(), "c".to_string()];
    let fused = rrf_fuse(&[list_a, list_b], 10);
    // "a" must appear exactly once
    assert_eq!(fused.iter().filter(|x| x.as_str() == "a").count(), 1);
}
