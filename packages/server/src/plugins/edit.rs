//! Matching engine for the sandbox_edit tool.
//!
//! Models frequently quote code with slightly wrong whitespace or
//! indentation. Rather than failing outright, the edit tool locates the
//! target text with a pipeline of increasingly forgiving strategies — but it
//! never guesses between multiple candidates: an ambiguous pattern is an
//! error, because replacing the wrong occurrence corrupts the file silently.

/// A located occurrence of the search pattern, as byte offsets into the file
/// content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchRange {
    /// Byte offset of the start of the match.
    pub start: usize,
    /// Byte offset one past the end of the match.
    pub end: usize,
}

/// A per-line normalizer used by the line-based matching strategies.
type LineNormalizer = fn(&str) -> String;

/// The outcome of running the matching pipeline.
#[derive(Debug)]
pub struct Matches {
    /// Which strategy produced the matches (e.g. "exact", "line_trimmed").
    pub strategy: &'static str,
    /// All non-overlapping occurrences, in file order.
    pub ranges: Vec<MatchRange>,
}

/// Locates `old` in `content`, trying each strategy in order and returning
/// the first one that matches anywhere. Strategies are ordered from strict to
/// forgiving so an exact match always wins.
pub fn find_matches(content: &str, old: &str) -> Option<Matches> {
    if old.is_empty() {
        return None;
    }
    let exact: Vec<MatchRange> = content
        .match_indices(old)
        .map(|(start, _)| MatchRange {
            start,
            end: start + old.len(),
        })
        .collect();
    if !exact.is_empty() {
        return Some(Matches {
            strategy: "exact",
            ranges: exact,
        });
    }

    // indentation_flexible preserves the block's relative indent structure,
    // so it must run before the per-line strategies that discard it — they
    // match a superset of what it matches.
    let ranges = indent_flexible_matches(content, old);
    if !ranges.is_empty() {
        return Some(Matches {
            strategy: "indentation_flexible",
            ranges,
        });
    }

    let strategies: [(&'static str, LineNormalizer); 2] = [
        ("line_trimmed", |line: &str| line.trim().to_string()),
        ("whitespace_normalized", |line: &str| {
            line.split_whitespace().collect::<Vec<_>>().join(" ")
        }),
    ];
    for (name, normalize) in strategies {
        let ranges = line_matches(content, old, normalize);
        if !ranges.is_empty() {
            return Some(Matches {
                strategy: name,
                ranges,
            });
        }
    }
    None
}

/// Applies the replacement to every range (which must be sorted and
/// non-overlapping), returning the new content.
pub fn apply_replacements(content: &str, ranges: &[MatchRange], new: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let mut cursor = 0;
    for range in ranges {
        result.push_str(&content[cursor..range.start]);
        result.push_str(new);
        cursor = range.end;
    }
    result.push_str(&content[cursor..]);
    result
}

/// Splits the pattern into lines for line-wise matching. A single trailing
/// newline is dropped (it would otherwise produce a phantom empty last line);
/// the caller compensates by extending the matched range over the newline.
fn pattern_lines(old: &str) -> Vec<&str> {
    let trimmed = old.strip_suffix('\n').unwrap_or(old);
    trimmed.split('\n').collect()
}

/// Byte ranges of each line in `content`, excluding line terminators.
fn content_line_spans(content: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = 0;
    for line in content.split('\n') {
        spans.push((start, start + line.len()));
        start += line.len() + 1;
    }
    spans
}

/// If the pattern carried a trailing newline and the matched region is
/// followed by one in the file, include it so replacements with trailing
/// newlines don't introduce blank lines.
fn extend_over_newline(content: &str, old: &str, mut range: MatchRange) -> MatchRange {
    if old.ends_with('\n') && content.as_bytes().get(range.end) == Some(&b'\n') {
        range.end += 1;
    }
    range
}

/// Finds windows of content lines that equal the pattern lines after
/// applying `normalize` to both sides.
fn line_matches(content: &str, old: &str, normalize: fn(&str) -> String) -> Vec<MatchRange> {
    let old_lines: Vec<String> = pattern_lines(old).iter().map(|l| normalize(l)).collect();
    if old_lines.is_empty() {
        return Vec::new();
    }
    let lines: Vec<&str> = content.split('\n').collect();
    let spans = content_line_spans(content);
    let normalized: Vec<String> = lines.iter().map(|l| normalize(l)).collect();

    collect_window_matches(
        content,
        old,
        &spans,
        |i| normalized[i..i + old_lines.len()] == old_lines[..],
        old_lines.len(),
        lines.len(),
    )
}

/// Finds windows whose lines equal the pattern lines after stripping each
/// block's own common indentation (so a correctly shaped block matches even
/// when the model got the absolute indent level wrong). Trailing whitespace
/// is ignored per line.
fn indent_flexible_matches(content: &str, old: &str) -> Vec<MatchRange> {
    let old_block = deindent(&pattern_lines(old));
    if old_block.is_empty() {
        return Vec::new();
    }
    let lines: Vec<&str> = content.split('\n').collect();
    let spans = content_line_spans(content);

    collect_window_matches(
        content,
        old,
        &spans,
        |i| deindent(&lines[i..i + old_block.len()]) == old_block,
        old_block.len(),
        lines.len(),
    )
}

/// Shared window-scan: applies `window_eq` to every window position and
/// returns the non-overlapping match ranges (first match wins on overlap).
fn collect_window_matches(
    content: &str,
    old: &str,
    spans: &[(usize, usize)],
    window_eq: impl Fn(usize) -> bool,
    window_len: usize,
    total_lines: usize,
) -> Vec<MatchRange> {
    let mut ranges: Vec<MatchRange> = Vec::new();
    let mut i = 0;
    while i + window_len <= total_lines {
        if window_eq(i) {
            let range = MatchRange {
                start: spans[i].0,
                end: spans[i + window_len - 1].1,
            };
            ranges.push(extend_over_newline(content, old, range));
            i += window_len;
        } else {
            i += 1;
        }
    }
    ranges
}

/// Removes the block's common leading indentation and per-line trailing
/// whitespace. Blank lines don't count towards the common indent.
fn deindent(lines: &[&str]) -> Vec<String> {
    let min_indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|line| {
            if line.len() >= min_indent && line.is_char_boundary(min_indent) {
                line[min_indent..].trim_end().to_string()
            } else {
                line.trim_end().to_string()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single(content: &str, old: &str) -> (String, &'static str) {
        let matches = find_matches(content, old).expect("expected a match");
        assert_eq!(matches.ranges.len(), 1, "expected exactly one match");
        (
            apply_replacements(content, &matches.ranges, "REPLACED"),
            matches.strategy,
        )
    }

    #[test]
    fn exact_match_wins() {
        let (out, strategy) = single("let x = 1;\nlet y = 2;\n", "let y = 2;");
        assert_eq!(strategy, "exact");
        assert_eq!(out, "let x = 1;\nREPLACED\n");
    }

    #[test]
    fn exact_reports_all_occurrences() {
        let matches = find_matches("a\nb\na\n", "a").unwrap();
        assert_eq!(matches.strategy, "exact");
        assert_eq!(matches.ranges.len(), 2);
    }

    #[test]
    fn line_trimmed_recovers_when_relative_indent_is_wrong() {
        // Tab-indented file, space-indented pattern with flattened relative
        // indent: exact and indentation_flexible both fail, line_trimmed
        // recovers.
        let content = "fn f() {\n\ta();\n\t\tb();\n}\n";
        let (out, strategy) = single(content, "a();\nb();");
        assert_eq!(strategy, "line_trimmed");
        assert_eq!(out, "fn f() {\nREPLACED\n}\n");
    }

    #[test]
    fn whitespace_normalized_recovers_from_inner_whitespace() {
        let content = "let x\t=\t1;\n";
        let (out, strategy) = single(content, "let x = 1;");
        assert_eq!(strategy, "whitespace_normalized");
        assert_eq!(out, "REPLACED\n");
    }

    #[test]
    fn indent_flexible_matches_shifted_blocks() {
        // Same block shape and indent style, quoted at the wrong indent
        // level: relative structure is preserved, so indentation_flexible
        // fires before the line-based strategies.
        let content = "fn main() {\n        if a {\n            b();\n        }\n}\n";
        let old = "if a {\n    b();\n}";
        let (out, strategy) = single(content, old);
        assert_eq!(strategy, "indentation_flexible");
        assert_eq!(out, "fn main() {\nREPLACED\n}\n");
    }

    #[test]
    fn multiline_with_trailing_newline_does_not_leave_blank_line() {
        // Tab vs space indent forces a line-based match; the pattern's
        // trailing newline must extend the matched range over the file's
        // newline so the replacement doesn't leave a blank line behind.
        let content = "before\n\told line\nafter\n";
        let matches = find_matches(content, "  old line\n").unwrap();
        assert_eq!(matches.strategy, "indentation_flexible");
        let out = apply_replacements(content, &matches.ranges, "new line\n");
        assert_eq!(out, "before\nnew line\nafter\n");
    }

    #[test]
    fn multi_line_windows_require_all_lines() {
        let content = "\tfoo();\nbar()\t;\n\tfoo();\n";
        // Two-line pattern: only the first window matches both lines.
        let matches = find_matches(content, "foo();\nbar() ;").unwrap();
        assert_eq!(matches.ranges.len(), 1);

        // Single-line pattern matches both indented occurrences.
        let matches = find_matches(content, "  foo();").unwrap();
        assert_eq!(matches.ranges.len(), 2);
    }

    #[test]
    fn no_match_returns_none() {
        assert!(find_matches("alpha\nbeta\n", "gamma").is_none());
        assert!(find_matches("alpha", "").is_none());
    }

    #[test]
    fn replace_all_applies_every_range() {
        let matches = find_matches("x=1; x=1; x=1;", "x=1;").unwrap();
        assert_eq!(matches.ranges.len(), 3);
        let out = apply_replacements("x=1; x=1; x=1;", &matches.ranges, "y=2;");
        assert_eq!(out, "y=2; y=2; y=2;");
    }

    #[test]
    fn overlapping_windows_do_not_double_match() {
        // "a\na" twice over "a\na\na": windows at 0 and 1 overlap; only the
        // first is taken, then scanning resumes past it.
        let matches = find_matches("a\na\na", " a\n a").unwrap();
        assert_eq!(matches.ranges.len(), 1);
    }
}
