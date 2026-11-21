//! CLI tests for running several queries in one pass (`-e`/`--query`,
//! `--with-query`).
//!
//! Exact output is asserted only under `--porcelain` (no colors, one
//! compact JSON value per line).

use assert_cmd::Command;
use std::io::Write as _;

/// Path to the `simple.json` fixture.
const SIMPLE_JSON_FILEPATH: &str = "tests/data/simple/simple.json";

/// Document used by most stdin tests.
const USERS_DOC: &str = r#"{"users": [{"name": "Ada", "age": 36}]}"#;

/// Two users; used for `--max-count` and counting tests.
const TWO_USERS_DOC: &str =
    r#"{"users": [{"name": "Ada", "age": 36}, {"name": "Bob"}]}"#;

/// Run `jg` with `args`, feeding `stdin`.
fn run_stdin(args: &[&str], stdin: &str) -> assert_cmd::assert::Assert {
    let mut cmd = Command::cargo_bin("jg").expect("Failed to find main binary");
    cmd.args(args).write_stdin(stdin.to_string()).assert()
}

/// Run `jg` with `args` and no stdin.
fn run(args: &[&str]) -> assert_cmd::assert::Assert {
    let mut cmd = Command::cargo_bin("jg").expect("Failed to find main binary");
    cmd.args(args).assert()
}

/// Captured stdout as UTF-8.
fn stdout_of(assert: &assert_cmd::assert::Assert) -> String {
    String::from_utf8(assert.get_output().stdout.clone())
        .expect("Invalid UTF-8 output")
}

/// Write `data` into a temporary file with the given `suffix`.
fn temp_file_with(suffix: &str, data: &[u8]) -> tempfile::NamedTempFile {
    let mut f = tempfile::Builder::new()
        .suffix(suffix)
        .tempfile()
        .expect("create temp file");
    f.write_all(data).expect("write temp file");
    f.flush().expect("flush temp file");
    f
}

/// Path of a temporary file as a string.
fn path_of(f: &tempfile::NamedTempFile) -> String {
    f.path().to_str().expect("utf-8 temp path").to_string()
}

// ---------------------------------------------------------------------------
// Repeated -e and plain output
// ---------------------------------------------------------------------------

/// Run the same invocation twice, once with the query as the positional
/// argument and once via `-e`, and return both outputs. `QUERY` in
/// `template` marks where the query goes.
fn single_and_flag_outputs(
    template: &[&str],
    query: &str,
    stdin: Option<&str>,
) -> (String, String) {
    let positional: Vec<&str> = template
        .iter()
        .map(|a| if *a == "QUERY" { query } else { a })
        .collect();
    let mut flagged: Vec<&str> = vec!["-e", query];
    flagged.extend(template.iter().filter(|a| **a != "QUERY"));
    let go = |args: &[&str]| {
        stdin.map_or_else(
            || run(args).success().code(0),
            |input| run_stdin(args, input).success().code(0),
        )
    };

    (stdout_of(&go(&positional)), stdout_of(&go(&flagged)))
}

#[test]
fn repeated_query_flag_prints_value_matched_twice_once() {
    let a = run_stdin(
        &["-e", "users[*].name", "-e", "users[*].*", "--porcelain"],
        USERS_DOC,
    )
    .success()
    .code(0);
    assert_eq!(stdout_of(&a), "\"Ada\"\n36\n");
}

#[test]
fn long_query_flag_is_repeatable() {
    let a = run_stdin(
        &["--query", "users[*].age", "--query", "users[*].name", "--porcelain"],
        USERS_DOC,
    )
    .success();
    assert_eq!(stdout_of(&a), "\"Ada\"\n36\n");
}

#[test]
fn positional_argument_is_a_file_when_query_flag_given() {
    let f = temp_file_with(".json", br#"{"name": "from-file"}"#);
    let path = path_of(&f);
    let a = run(&["-e", "name", &path, "--porcelain"]).success();
    assert_eq!(stdout_of(&a), "\"from-file\"\n");
}

#[test]
fn plain_count_counts_value_matched_by_several_queries_once() {
    let a = run_stdin(
        &["-e", "users[*].name", "-e", "users[*].*", "--count", "--porcelain"],
        USERS_DOC,
    )
    .success();
    assert_eq!(stdout_of(&a), "2\n");
}

#[test]
fn with_query_prints_query_indices_before_each_value() {
    let a = run_stdin(
        &[
            "-e",
            "users[*].name",
            "-e",
            "users[*].*",
            "--with-query",
            "--porcelain",
        ],
        USERS_DOC,
    )
    .success()
    .code(0);
    assert_eq!(stdout_of(&a), "0,1:\n\"Ada\"\n1:\n36\n");
}

#[test]
fn with_query_and_with_path_combine_indices_and_path() {
    let a = run_stdin(
        &[
            "-e",
            "users[*].name",
            "-e",
            "users[*].*",
            "--with-query",
            "--with-path",
            "--porcelain",
        ],
        USERS_DOC,
    )
    .success();
    assert_eq!(
        stdout_of(&a),
        "0,1:users.[0].name:\n\"Ada\"\n1:users.[0].age:\n36\n"
    );
}

#[test]
fn with_query_on_positional_query_uses_index_zero() {
    let f = temp_file_with(".json", USERS_DOC.as_bytes());
    let path = path_of(&f);
    let a =
        run(&["--with-query", "--porcelain", "users[*].name", &path]).success();
    assert_eq!(stdout_of(&a), "0:\n\"Ada\"\n");
}

#[test]
fn with_query_lists_indices_in_ascending_numeric_order() {
    // Eleven queries; queries 2 and 10 both match `b`.
    let mut args: Vec<String> = Vec::new();
    for q in ["a", "x0", "b", "x1", "x2", "x3", "x4", "x5", "x6", "x7", "b"] {
        args.push("-e".to_string());
        args.push(q.to_string());
    }
    args.push("--with-query".to_string());
    args.push("--porcelain".to_string());
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let a = run_stdin(&refs, r#"{"a": 1, "b": 2}"#).success();
    assert_eq!(stdout_of(&a), "0:\n1\n2,10:\n2\n");
}

#[test]
fn count_with_query_prints_query_count_lines_including_zero() {
    let a = run_stdin(
        &[
            "-e",
            "users[*].name",
            "-e",
            "users[*].*",
            "-e",
            "nowhere",
            "--with-query",
            "--count",
            "--porcelain",
        ],
        USERS_DOC,
    )
    .success();
    assert_eq!(stdout_of(&a), "0:1\n1:2\n2:0\n");
}

#[test]
fn count_with_query_over_several_files_prefixes_file_names() {
    let a_file = temp_file_with(".json", br#"{"x": 1, "y": 2}"#);
    let b_file = temp_file_with(".json", br#"{"z": 3}"#);
    let (a_path, b_path) = (path_of(&a_file), path_of(&b_file));
    let a = run(&[
        "-e",
        "x",
        "-e",
        "*",
        "--with-query",
        "--count",
        "--porcelain",
        &a_path,
        &b_path,
    ])
    .success();
    assert_eq!(
        stdout_of(&a),
        format!("{a_path}:0:1\n{a_path}:1:2\n{b_path}:0:0\n{b_path}:1:1\n")
    );
}

#[test]
fn files_with_matches_with_query_lists_matching_queries_per_file() {
    let a_file = temp_file_with(".json", br#"{"x": 1, "z": 3}"#);
    let b_file = temp_file_with(".json", br#"{"other": 1}"#);
    let c_file = temp_file_with(".json", br#"{"y": 2}"#);
    let (a_path, b_path, c_path) =
        (path_of(&a_file), path_of(&b_file), path_of(&c_file));
    let a = run(&[
        "-e",
        "x",
        "-e",
        "y",
        "-e",
        "z",
        "--with-query",
        "-l",
        "--porcelain",
        &a_path,
        &b_path,
        &c_path,
    ])
    .success()
    .code(0);
    assert_eq!(stdout_of(&a), format!("{a_path}:0,2\n{c_path}:1\n"));
}

#[test]
fn files_with_matches_with_query_prints_nothing_without_matches() {
    let a_file = temp_file_with(".json", br#"{"other": 1}"#);
    let b_file = temp_file_with(".json", br#"{"more": 2}"#);
    let (a_path, b_path) = (path_of(&a_file), path_of(&b_file));
    let a = run(&[
        "-e",
        "x",
        "-e",
        "y",
        "--with-query",
        "-l",
        "--porcelain",
        &a_path,
        &b_path,
    ])
    .failure()
    .code(1);
    assert_eq!(stdout_of(&a), "");
}

#[test]
fn max_count_with_query_prints_first_values_with_full_query_lists() {
    let a = run_stdin(
        &[
            "-e",
            "users[*].name",
            "-e",
            "users[*].*",
            "-m",
            "2",
            "--with-query",
            "--porcelain",
        ],
        TWO_USERS_DOC,
    )
    .success();
    assert_eq!(stdout_of(&a), "0,1:\n\"Ada\"\n1:\n36\n");
}

#[test]
fn max_count_with_query_count_covers_only_first_values() {
    let a = run_stdin(
        &[
            "-e",
            "users[*].name",
            "-e",
            "users[*].*",
            "-e",
            "nowhere",
            "-m",
            "2",
            "--with-query",
            "--count",
            "--porcelain",
        ],
        TWO_USERS_DOC,
    )
    .success();
    assert_eq!(stdout_of(&a), "0:1\n1:2\n2:0\n");
}

#[test]
fn max_count_plain_count_is_capped_by_distinct_values() {
    let capped = run_stdin(
        &[
            "-e",
            "users[*].name",
            "-e",
            "users[*].*",
            "-m",
            "2",
            "--count",
            "--porcelain",
        ],
        TWO_USERS_DOC,
    )
    .success();
    assert_eq!(stdout_of(&capped), "2\n");
    let uncapped = run_stdin(
        &[
            "-e",
            "users[*].name",
            "-e",
            "users[*].*",
            "-m",
            "10",
            "--count",
            "--porcelain",
        ],
        TWO_USERS_DOC,
    )
    .success();
    assert_eq!(stdout_of(&uncapped), "3\n");
}

#[test]
fn ignore_case_applies_to_every_query() {
    let a = run_stdin(
        &["-e", "name", "-e", "age", "-i", "--with-query", "--porcelain"],
        r#"{"NAME": "x", "Age": 3}"#,
    )
    .success();
    assert_eq!(stdout_of(&a), "0:\n\"x\"\n1:\n3\n");
}

#[test]
fn fixed_string_applies_to_every_query() {
    let a = run_stdin(
        &["-e", "a.b", "-e", "id", "-F", "--with-query", "--porcelain"],
        r#"{"outer": {"a.b": 1, "list": [{"id": 7}]}, "a": {"b": 2}}"#,
    )
    .success();
    assert_eq!(stdout_of(&a), "0:\n1\n1:\n7\n");
}

#[test]
fn with_query_conflicts_with_depth() {
    // Without --depth the same query set is accepted and attributed.
    let ok = run(&[
        "-e",
        "age",
        "--with-query",
        "--porcelain",
        SIMPLE_JSON_FILEPATH,
    ])
    .success()
    .code(0);
    assert_eq!(
        stdout_of(&ok),
        "0:
32
"
    );
    // --depth on its own is a valid invocation.
    run(&["--depth", "--porcelain", SIMPLE_JSON_FILEPATH]).success();
    // Only the combination is rejected, as a usage error.
    run(&["-e", "age", "--with-query", "--depth", SIMPLE_JSON_FILEPATH])
        .failure()
        .code(2);
}

#[test]
fn with_query_conflicts_with_files() {
    // Without --files the same query set is accepted and attributed.
    let ok = run(&[
        "-e",
        "age",
        "--with-query",
        "--porcelain",
        SIMPLE_JSON_FILEPATH,
    ])
    .success()
    .code(0);
    assert_eq!(
        stdout_of(&ok),
        "0:
32
"
    );
    // --files on its own is a valid invocation.
    run(&["--files", "--porcelain", SIMPLE_JSON_FILEPATH]).success();
    // Only the combination is rejected, as a usage error.
    run(&["-e", "age", "--with-query", "--files", SIMPLE_JSON_FILEPATH])
        .failure()
        .code(2);
}

#[test]
fn unparsable_query_flag_exits_two() {
    let ok = run_stdin(
        &["-e", "a", "-e", "b", "-e", "c", "--porcelain"],
        r#"{"a": 1, "b": 2, "c": 3}"#,
    )
    .success()
    .code(0);
    assert_eq!(
        stdout_of(&ok),
        "1
2
3
"
    );
    // One unparsable query among valid ones fails the whole invocation.
    run_stdin(
        &["-e", "a", "-e", "unclosed\"", "-e", "c", "--porcelain"],
        r#"{"a": 1, "b": 2, "c": 3}"#,
    )
    .failure()
    .code(2);
}

#[test]
fn exit_status_reflects_any_query_matching() {
    run_stdin(&["-e", "nope", "-e", "a", "--porcelain"], r#"{"a": 1}"#)
        .success()
        .code(0);
    let a = run_stdin(
        &["-e", "nope", "-e", "missing", "--porcelain"],
        r#"{"a": 1}"#,
    )
    .failure()
    .code(1);
    assert_eq!(stdout_of(&a), "");
}

#[test]
fn yaml_file_with_several_queries() {
    let f = temp_file_with(".yaml", b"users:\n  - name: Ada\n    age: 36\n");
    let path = path_of(&f);
    let a = run(&[
        "-e",
        "users[*].name",
        "-e",
        "users[*].*",
        "--with-query",
        "--porcelain",
        &path,
    ])
    .success();
    assert_eq!(stdout_of(&a), "0,1:\n\"Ada\"\n1:\n36\n");
}

#[test]
fn single_query_porcelain_output_unchanged() {
    let (positional, flagged) = single_and_flag_outputs(
        &["--porcelain", "--no-path", "QUERY"],
        "users[*]",
        Some(r#"{"users": [{"a": 1}, {"b": 2}]}"#),
    );
    assert_eq!(
        positional,
        "{\"a\":1}
{\"b\":2}
"
    );
    assert_eq!(flagged, positional);
}

#[test]
fn single_query_count_output_unchanged() {
    let (positional, flagged) = single_and_flag_outputs(
        &["QUERY", SIMPLE_JSON_FILEPATH, "--count", "--porcelain"],
        "name.*",
        None,
    );
    assert_eq!(
        positional,
        "2
"
    );
    assert_eq!(flagged, positional);

    let a_file = temp_file_with(".json", br#"{"x": 1, "y": 2}"#);
    let b_file = temp_file_with(".json", br#"{"z": 3}"#);
    let (a_path, b_path) = (path_of(&a_file), path_of(&b_file));
    let (positional, flagged) = single_and_flag_outputs(
        &["QUERY", &a_path, &b_path, "--count", "--porcelain"],
        "*",
        None,
    );
    assert_eq!(
        positional,
        format!(
            "{a_path}:2
{b_path}:1
"
        )
    );
    assert_eq!(flagged, positional);
}
