//! Library tests for running several queries over one document with
//! `QueryDFASet`.
//!
//! Expected results are derived from the existing single-query engine:
//! the order and pointers of a set's matches must equal those of
//! `QueryDFA::find` for the union `(q0) | (q1) | ...`, and query `i` is
//! attributed to a match exactly when `QueryDFA::find` for `q_i` alone
//! returns a pointer with the same path.

use std::ops::ControlFlow;

use jsongrep::Value;
use jsongrep::query::{JSONPointer, PathType, QueryDFA, QueryDFASet, SetMatch};

/// Parse a JSON document into the borrowed value type the engine consumes.
fn doc(s: &str) -> Value<'_> {
    serde_json::from_str(s).expect("test document must be valid JSON")
}

/// Render a path the way the CLI path header does, e.g. `users.[0].name`.
fn path_str(path: &[PathType]) -> String {
    path.iter().map(ToString::to_string).collect::<Vec<_>>().join(".")
}

/// Compact `(path, queries)` view of a set result for readable asserts.
fn summary(matches: &[SetMatch<'_>]) -> Vec<(String, Vec<usize>)> {
    matches
        .iter()
        .map(|m| (path_str(&m.pointer.path), m.queries.clone()))
        .collect()
}

/// Owned `(path, queries)` pairs from string literals.
fn expect_pairs(pairs: &[(&str, &[usize])]) -> Vec<(String, Vec<usize>)> {
    pairs.iter().map(|(p, q)| ((*p).to_string(), q.to_vec())).collect()
}

/// Matches of a single query, computed with the existing engine.
fn single<'a>(query: &str, json: &'a Value<'a>) -> Vec<JSONPointer<'a>> {
    QueryDFA::from_query_str(query)
        .expect("reference query must parse")
        .find(json)
}

/// Reference result built only from the single-query engine: the union
/// query fixes order and pointers, each single query fixes attribution.
fn reference<'a>(queries: &[&str], json: &'a Value<'a>) -> Vec<SetMatch<'a>> {
    let union = queries
        .iter()
        .map(|q| format!("({q})"))
        .collect::<Vec<_>>()
        .join(" | ");
    let singles: Vec<Vec<JSONPointer<'a>>> =
        queries.iter().map(|q| single(q, json)).collect();
    single(&union, json)
        .into_iter()
        .map(|pointer| {
            let queries = singles
                .iter()
                .enumerate()
                .filter(|(_, found)| {
                    found.iter().any(|p| p.path == pointer.path)
                })
                .map(|(i, _)| i)
                .collect();
            SetMatch { pointer, queries }
        })
        .collect()
}

/// Per-query counts from the single-query engine.
fn reference_counts(queries: &[&str], json: &Value<'_>) -> Vec<usize> {
    queries.iter().map(|q| single(q, json).len()).collect()
}

/// Tally the queries of the given matches, one count per query.
fn tally(matches: &[SetMatch<'_>], num_queries: usize) -> Vec<usize> {
    let mut counts = vec![0; num_queries];
    for m in matches {
        for &q in &m.queries {
            counts[q] += 1;
        }
    }
    counts
}

/// Build a set or panic with the offending queries.
fn set(queries: &[&str]) -> QueryDFASet {
    QueryDFASet::from_query_strs(queries)
        .unwrap_or_else(|e| panic!("queries {queries:?} must parse: {e:?}"))
}

/// Check every observable of the set against the single-query reference.
fn assert_agrees_with_single_queries(queries: &[&str], json: &Value<'_>) {
    let s = set(queries);
    let found = s.find(json);
    let expected = reference(queries, json);
    assert_eq!(found, expected, "find mismatch for queries {queries:?}");

    let counts = reference_counts(queries, json);
    assert_eq!(
        s.count_per_query(json),
        counts,
        "count_per_query for {queries:?}"
    );
    let matching: Vec<usize> =
        (0..queries.len()).filter(|&i| counts[i] > 0).collect();
    assert_eq!(
        s.matching_queries(json),
        matching,
        "matching_queries for {queries:?}"
    );

    for limit in 1..=expected.len() + 2 {
        let prefix = &expected[..limit.min(expected.len())];
        assert_eq!(
            s.find_limited(json, limit),
            prefix,
            "find_limited({limit}) for {queries:?}"
        );
        assert_eq!(
            s.count_per_query_limited(json, limit),
            tally(prefix, queries.len()),
            "count_per_query_limited({limit}) for {queries:?}"
        );
    }
}

const USERS: &str = r#"{"users":[{"name":"Ada","age":36},{"name":"Bob"}]}"#;

#[test]
fn find_reports_one_entry_per_value_with_all_matching_queries() {
    let json = doc(USERS);
    let s = set(&["users[*].name", "users[*].*", "nowhere"]);
    let found = s.find(&json);
    assert_eq!(
        summary(&found),
        expect_pairs(&[
            ("users.[0].name", &[0, 1]),
            ("users.[0].age", &[1]),
            ("users.[1].name", &[0, 1]),
        ])
    );
    assert_eq!(found[0].pointer.value.to_string(), r#""Ada""#);
    assert_eq!(found[1].pointer.value.to_string(), "36");
    assert_eq!(found[2].pointer.value.to_string(), r#""Bob""#);
}

#[test]
fn count_per_query_counts_each_query_independently() {
    let json = doc(USERS);
    let s = set(&["users[*].name", "users[*].*", "nowhere"]);
    assert_eq!(s.count_per_query(&json), vec![2, 3, 0]);
}

#[test]
fn matching_queries_lists_queries_with_any_match() {
    let json = doc(USERS);
    let s = set(&["users[*].name", "users[*].*", "nowhere"]);
    assert_eq!(s.matching_queries(&json), vec![0, 1]);
}

#[test]
fn find_order_and_pointers_equal_union_query_results() {
    let json = doc(r#"{"b":{"x":1,"a":2},"a":[3,{"x":4}],"x":5}"#);
    let queries = ["x", "b.*", "a[*]", "(* | [*])*.x"];
    let found = set(&queries).find(&json);
    let union = single("x | b.* | a[*] | (* | [*])*.x", &json);
    let pointers: Vec<JSONPointer<'_>> =
        found.iter().map(|m| m.pointer.clone()).collect();
    assert_eq!(pointers, union);
}

#[test]
fn matches_follow_document_order_not_query_order() {
    let json = doc(r#"{"first":1,"second":2,"third":3}"#);
    let found = set(&["third", "second", "first"]).find(&json);
    assert_eq!(
        summary(&found),
        expect_pairs(&[("first", &[2]), ("second", &[1]), ("third", &[0])])
    );
}

#[test]
fn query_indices_are_ascending_distinct_and_non_empty() {
    let json = doc(r#"{"a":{"a":{"a":1}},"b":[{"a":2},{"a":3}]}"#);
    let queries =
        ["(* | [*])*.a", "a", "a.a", "b[*].a", "*", "(* | [*])*", "b[1]"];
    for m in set(&queries).find(&json) {
        assert_ne!(m.queries, Vec::<usize>::new(), "queries must not be empty");
        assert!(
            m.queries.windows(2).all(|w| w[0] < w[1]),
            "indices must be strictly ascending: {:?}",
            m.queries
        );
        assert!(m.queries.iter().all(|&i| i < queries.len()));
    }
}

#[test]
fn overlapping_index_ranges_are_attributed_per_index() {
    let json = doc(r#"{"xs":[10,11,12,13,14]}"#);
    let found = set(&["xs[0:2]", "xs[1:4]", "xs[*]"]).find(&json);
    assert_eq!(
        summary(&found),
        expect_pairs(&[
            ("xs.[0]", &[0, 2]),
            ("xs.[1]", &[0, 1, 2]),
            ("xs.[2]", &[1, 2]),
            ("xs.[3]", &[1, 2]),
            ("xs.[4]", &[2]),
        ])
    );
}

#[test]
fn identical_queries_are_both_attributed() {
    let json = doc(r#"{"a":1,"b":2}"#);
    let s = set(&["a", "a"]);
    assert_eq!(summary(&s.find(&json)), expect_pairs(&[("a", &[0, 1])]));
    assert_eq!(s.count_per_query(&json), vec![1, 1]);
    assert_eq!(s.matching_queries(&json), vec![0, 1]);
}

#[test]
fn never_matching_query_gets_zero_count_and_no_attribution() {
    let json = doc(r#"{"a":1,"b":[1,2]}"#);
    let s = set(&["zzz", "a", "b[7]"]);
    assert_eq!(summary(&s.find(&json)), expect_pairs(&[("a", &[1])]));
    assert_eq!(s.count_per_query(&json), vec![0, 1, 0]);
    assert_eq!(s.matching_queries(&json), vec![1]);
}

#[test]
fn no_query_matching_yields_empty_results() {
    let json = doc("5");
    let s = set(&["a", "[0]", "*.b"]);
    assert_eq!(s.find(&json), Vec::<SetMatch<'_>>::new());
    assert_eq!(s.find_limited(&json, 3), Vec::<SetMatch<'_>>::new());
    assert_eq!(s.count_per_query(&json), vec![0, 0, 0]);
    assert_eq!(s.count_per_query_limited(&json, 3), vec![0, 0, 0]);
    assert_eq!(s.matching_queries(&json), Vec::<usize>::new());
    let visited = s.for_each_match(&json, |_| ControlFlow::<()>::Break(()));
    assert_eq!(visited, ControlFlow::Continue(()));
}

#[test]
fn recursive_descent_attributes_nested_names() {
    let json = doc(r#"{"name":"r","kids":[{"name":"k1"}]}"#);
    let found = set(&["(* | [*])*.name", "kids[*].name"]).find(&json);
    assert_eq!(
        summary(&found),
        expect_pairs(&[("name", &[0]), ("kids.[0].name", &[0, 1])])
    );
}

#[test]
fn optional_steps_agree_with_single_queries() {
    let json = doc(r#"{"a":{"b":1},"b":2,"c":{"a":{"b":3}}}"#);
    assert_agrees_with_single_queries(
        &["a?.b", "b", "c.a?.b", "(a | c).*"],
        &json,
    );
}

#[test]
fn many_queries_keep_numeric_index_order() {
    let keys: Vec<String> = (0..12).map(|i| format!("k{i}")).collect();
    let body = keys
        .iter()
        .enumerate()
        .map(|(i, k)| format!("\"{k}\":{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let text = format!("{{{body}}}");
    let json = doc(&text);
    let mut queries: Vec<String> = keys;
    queries.push("*".to_string());
    queries.push("k10".to_string());
    let s = QueryDFASet::from_query_strs(&queries).expect("queries parse");
    let found = s.find(&json);
    assert_eq!(found.len(), 12);
    for (i, m) in found.iter().enumerate() {
        let mut expected = vec![i, 12];
        if i == 10 {
            expected.push(13);
        }
        assert_eq!(m.queries, expected, "value k{i}");
    }
    let mut counts = vec![1; 12];
    counts.push(12);
    counts.push(1);
    assert_eq!(s.count_per_query(&json), counts);
}

#[test]
fn from_query_strs_accepts_owned_strings() {
    let json = doc(USERS);
    let queries: Vec<String> =
        vec!["users[*].name".to_string(), "users[0].age".to_string()];
    let s = QueryDFASet::from_query_strs(&queries).expect("queries parse");
    assert_eq!(
        summary(&s.find(&json)),
        expect_pairs(&[
            ("users.[0].name", &[0]),
            ("users.[0].age", &[1]),
            ("users.[1].name", &[0]),
        ])
    );
}

#[test]
fn unparsable_query_anywhere_in_the_list_is_an_error() {
    assert!(QueryDFASet::from_query_strs(&["unclosed\""]).is_err());
    assert!(QueryDFASet::from_query_strs(&["a", "b[", "c"]).is_err());
    assert!(QueryDFASet::from_query_strs(&["a", "b", "(c"]).is_err());
    assert!(QueryDFASet::from_query_strs(&["/re/", "a"]).is_err());
}

#[test]
fn find_limited_counts_values_not_query_hits() {
    let json = doc(USERS);
    let s = set(&["users[*].name", "users[*].*", "nowhere"]);
    let limited = s.find_limited(&json, 1);
    assert_eq!(summary(&limited), expect_pairs(&[("users.[0].name", &[0, 1])]));
}

#[test]
fn for_each_match_break_stops_and_returns_value() {
    let json = doc(r#"{"a":{"b":1,"c":2},"d":[3,{"b":4}]}"#);
    let s = set(&["(* | [*])*.b", "a.*", "d[*]"]);
    let all = s.find(&json);
    assert!(all.len() > 2);
    let mut visits = 0usize;
    let flow = s.for_each_match(&json, |m| {
        visits += 1;
        if visits == 2 {
            ControlFlow::Break((path_str(&m.pointer.path), m.queries))
        } else {
            ControlFlow::Continue(())
        }
    });
    assert_eq!(visits, 2, "visitor must not be called after Break");
    assert_eq!(
        flow,
        ControlFlow::Break((
            path_str(&all[1].pointer.path),
            all[1].queries.clone()
        ))
    );
}

#[test]
fn count_per_query_limited_tallies_first_values_only() {
    let json = doc(USERS);
    let s = set(&["users[*].name", "users[*].*", "nowhere"]);
    assert_eq!(s.count_per_query_limited(&json, 1), vec![1, 1, 0]);
    assert_eq!(s.count_per_query_limited(&json, 2), vec![1, 2, 0]);
    assert_eq!(s.count_per_query_limited(&json, 3), vec![2, 3, 0]);
    assert_eq!(s.count_per_query_limited(&json, 100), vec![2, 3, 0]);
}

#[test]
fn set_is_reusable_across_documents() {
    let s = set(&["name", "*.name", "[*].name"]);
    let first = doc(r#"{"name":"a","inner":{"name":"b"}}"#);
    let second = doc(r#"[{"name":"c"},{"other":1}]"#);
    assert_eq!(
        summary(&s.find(&first)),
        expect_pairs(&[("name", &[0]), ("inner.name", &[1])])
    );
    assert_eq!(summary(&s.find(&second)), expect_pairs(&[("[0].name", &[2])]));
    assert_eq!(s.matching_queries(&first), vec![0, 1]);
    assert_eq!(s.matching_queries(&second), vec![2]);
}
