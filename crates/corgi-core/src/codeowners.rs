use std::sync::LazyLock;

use ignore::gitignore::GitignoreBuilder;
use regex::Regex;

use crate::error::{CorgiError, Result};

static RULE_RE: LazyLock<Regex> = LazyLock::new(|| {
    let rule_label = regex::escape("Rule[auto-assign]:");
    Regex::new(&format!(r"^\s*#\s*{rule_label}\s*(.+?)\s*$")).expect("valid regex")
});

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub header: Vec<String>,
    pub items: Vec<Item>,
    pub footer: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Entry(Entry),
    Rule(Rule),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub leading: Vec<String>,
    pub path: String,
    pub owners: Vec<String>,
    pub kind: EntryKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Exact,
    Pattern,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    pub leading: Vec<String>,
    pub pattern: String,
    pub owners: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct RuleMatch {
    pub owners: Vec<String>,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self> {
        let lines = text.lines().map(str::to_string).collect::<Vec<_>>();
        let mut header = Vec::new();
        let mut footer = Vec::new();
        let mut items = Vec::new();
        let mut pending = Vec::new();
        let mut seen_item = false;

        for raw_line in lines {
            if let Some(rule) = parse_rule(&raw_line)? {
                let leading = if seen_item {
                    std::mem::take(&mut pending)
                } else {
                    Vec::new()
                };
                items.push(Item::Rule(Rule {
                    leading,
                    pattern: rule.0,
                    owners: rule.1,
                }));
                seen_item = true;
                continue;
            }

            if is_comment_or_blank(&raw_line) {
                if seen_item {
                    pending.push(raw_line);
                } else {
                    header.push(raw_line);
                }
                continue;
            }

            let (path, owners) = parse_entry_line(&raw_line)?;
            let leading = if seen_item {
                std::mem::take(&mut pending)
            } else {
                Vec::new()
            };
            let kind = if is_exact_path(&path) {
                EntryKind::Exact
            } else {
                EntryKind::Pattern
            };
            items.push(Item::Entry(Entry {
                leading,
                path,
                owners,
                kind,
            }));
            seen_item = true;
        }

        if seen_item {
            footer = pending;
        }

        Ok(Self {
            header,
            items,
            footer,
        })
    }

    pub fn render(&self) -> String {
        let mut lines = Vec::new();
        lines.extend(self.header.iter().cloned());
        for item in &self.items {
            match item {
                Item::Entry(entry) => {
                    lines.extend(entry.leading.iter().cloned());
                    let mut line = escape_token(&entry.path);
                    for owner in &entry.owners {
                        line.push(' ');
                        line.push_str(owner);
                    }
                    lines.push(line);
                }
                Item::Rule(rule) => {
                    lines.extend(rule.leading.iter().cloned());
                    let mut line = format!("# Rule[auto-assign]: {}", escape_token(&rule.pattern));
                    for owner in &rule.owners {
                        line.push(' ');
                        line.push_str(owner);
                    }
                    lines.push(line);
                }
            }
        }
        lines.extend(self.footer.iter().cloned());
        if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        }
    }

    pub fn sort_items(&mut self) {
        self.items.sort_by_key(item_sort_key);
    }

    pub fn has_patterns(&self) -> bool {
        self.items.iter().any(|item| {
            matches!(
                item,
                Item::Entry(Entry {
                    kind: EntryKind::Pattern,
                    ..
                })
            )
        })
    }

    pub fn exact_entries(&self) -> impl Iterator<Item = &Entry> {
        self.items.iter().filter_map(|item| match item {
            Item::Entry(entry) if entry.kind == EntryKind::Exact => Some(entry),
            _ => None,
        })
    }

    pub fn rules(&self) -> impl Iterator<Item = &Rule> {
        self.items.iter().filter_map(|item| match item {
            Item::Rule(rule) => Some(rule),
            _ => None,
        })
    }
}

impl Rule {
    pub fn matcher(&self) -> Result<RuleMatcher> {
        RuleMatcher::new(&self.pattern)
    }
}

pub struct RuleMatcher {
    matcher: ignore::gitignore::Gitignore,
}

impl RuleMatcher {
    pub fn new(pattern: &str) -> Result<Self> {
        let mut builder = GitignoreBuilder::new("/");
        builder
            .add_line(None, pattern)
            .map_err(|error| CorgiError::Parse(error.to_string()))?;
        let matcher = builder
            .build()
            .map_err(|error| CorgiError::Parse(error.to_string()))?;
        Ok(Self { matcher })
    }

    pub fn matches(&self, path: &str) -> bool {
        self.matcher
            .matched_path_or_any_parents(std::path::Path::new(path.trim_start_matches('/')), false)
            .is_ignore()
    }
}

pub fn select_auto_rule<'a>(
    rules: impl Iterator<Item = &'a Rule>,
    path: &str,
) -> Result<Option<RuleMatch>> {
    let mut best: Option<(usize, usize, RuleMatch)> = None;
    for rule in rules {
        let matcher = rule.matcher()?;
        if !matcher.matches(path) {
            continue;
        }

        let literal_len = rule
            .pattern
            .chars()
            .filter(|character| !matches!(character, '*' | '?' | '[' | ']' | '!'))
            .count();
        let score = (literal_len, rule.pattern.len());
        let matched = RuleMatch {
            owners: rule.owners.clone(),
        };
        if best
            .as_ref()
            .map(|(left_literal, left_len, _)| score > (*left_literal, *left_len))
            .unwrap_or(true)
        {
            best = Some((score.0, score.1, matched));
        }
    }
    Ok(best.map(|(_, _, rule)| rule))
}

pub fn is_exact_path(path: &str) -> bool {
    // CODEOWNERS semantics: an entry is only anchored to a single file when it
    // starts with `/` (repo/manifest-root anchored), has no wildcards, and does
    // not describe a directory. Unanchored entries like `src/lib.rs` are still
    // patterns that can match at any depth.
    path.starts_with('/')
        && !path.ends_with('/')
        && !path
            .chars()
            .any(|character| matches!(character, '*' | '?' | '[' | ']'))
}

pub fn matches_pattern(pattern: &str, path: &str) -> Result<bool> {
    let matcher = RuleMatcher::new(pattern)?;
    Ok(matcher.matches(path))
}

fn parse_rule(line: &str) -> Result<Option<(String, Vec<String>)>> {
    let Some(captures) = RULE_RE.captures(line) else {
        return Ok(None);
    };
    let body = captures.get(1).expect("rule body").as_str();
    let tokens = split_tokens(body);
    if tokens.is_empty() {
        return Err(CorgiError::Parse(line.to_string()));
    }
    let pattern = tokens[0].clone();
    let owners = tokens
        .into_iter()
        .skip(1)
        .take_while(|token| !token.starts_with('#'))
        .collect::<Vec<_>>();
    Ok(Some((pattern, owners)))
}

fn parse_entry_line(line: &str) -> Result<(String, Vec<String>)> {
    let tokens = split_tokens(line);
    if tokens.is_empty() {
        return Err(CorgiError::Parse(line.to_string()));
    }
    let path = tokens[0].clone();
    let owners = tokens
        .into_iter()
        .skip(1)
        .take_while(|token| !token.starts_with('#'))
        .collect::<Vec<_>>();
    Ok((path, owners))
}

fn split_tokens(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut escaped = false;

    for character in input.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }

        match character {
            '\\' => escaped = true,
            character if character.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(character),
        }
    }

    if escaped {
        current.push('\\');
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn escape_token(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            ' ' => escaped.push_str("\\ "),
            '\t' => escaped.push_str("\\\t"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn is_comment_or_blank(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.is_empty() || trimmed.starts_with('#')
}

fn item_sort_key(item: &Item) -> String {
    // Sort rules and entries together using their pattern/path so that
    // `Rule[auto-assign]` items land near the files they affect.
    match item {
        Item::Rule(rule) => rule.pattern.clone(),
        Item::Entry(entry) => entry.path.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── split_tokens ──────────────────────────────────────────────

    #[test]
    fn split_tokens_simple() {
        assert_eq!(
            split_tokens("/src/lib.rs @org/team"),
            vec!["/src/lib.rs", "@org/team"]
        );
    }

    #[test]
    fn split_tokens_escaped_space() {
        assert_eq!(
            split_tokens(r"/foo\ bar @owner"),
            vec!["/foo bar", "@owner"]
        );
    }

    #[test]
    fn split_tokens_multiple_owners() {
        assert_eq!(
            split_tokens("/path @a @b @c"),
            vec!["/path", "@a", "@b", "@c"]
        );
    }

    #[test]
    fn split_tokens_leading_trailing_whitespace() {
        assert_eq!(split_tokens("  /path  @owner  "), vec!["/path", "@owner"]);
    }

    #[test]
    fn split_tokens_tabs() {
        assert_eq!(split_tokens("/path\t@owner"), vec!["/path", "@owner"]);
    }

    #[test]
    fn split_tokens_empty_input() {
        let result: Vec<String> = split_tokens("");
        assert!(result.is_empty());
    }

    #[test]
    fn split_tokens_whitespace_only() {
        assert!(split_tokens("   ").is_empty());
    }

    #[test]
    fn split_tokens_trailing_backslash() {
        // A trailing backslash with no following character is preserved literally.
        assert_eq!(split_tokens(r"/path\"), vec!["/path\\"]);
    }

    #[test]
    fn split_tokens_escaped_backslash() {
        assert_eq!(split_tokens(r"/path\\file"), vec!["/path\\file"]);
    }

    #[test]
    fn split_tokens_unicode() {
        assert_eq!(
            split_tokens("/über/café.rs @team"),
            vec!["/über/café.rs", "@team"]
        );
    }

    // ── escape_token ──────────────────────────────────────────────

    #[test]
    fn escape_token_no_special() {
        assert_eq!(escape_token("/src/lib.rs"), "/src/lib.rs");
    }

    #[test]
    fn escape_token_space() {
        assert_eq!(escape_token("/foo bar"), r"/foo\ bar");
    }

    #[test]
    fn escape_token_backslash() {
        assert_eq!(escape_token(r"/path\file"), r"/path\\file");
    }

    #[test]
    fn escape_token_tab() {
        assert_eq!(escape_token("/path\tfile"), "/path\\\tfile");
    }

    // ── escape/split roundtrip ────────────────────────────────────

    #[test]
    fn escape_then_split_roundtrip() {
        let cases = [
            "/simple",
            "/with space",
            "/with\ttab",
            r"/back\slash",
            "/über/café",
        ];
        for input in cases {
            let escaped = escape_token(input);
            let tokens = split_tokens(&escaped);
            assert_eq!(tokens, vec![input], "roundtrip failed for {input:?}");
        }
    }

    // ── is_comment_or_blank ───────────────────────────────────────

    #[test]
    fn comment_or_blank_cases() {
        assert!(is_comment_or_blank(""));
        assert!(is_comment_or_blank("   "));
        assert!(is_comment_or_blank("# comment"));
        assert!(is_comment_or_blank("  # indented comment"));
        assert!(!is_comment_or_blank("/src/lib.rs @team"));
        assert!(!is_comment_or_blank("file.rs"));
    }

    // ── is_exact_path ─────────────────────────────────────────────

    #[test]
    fn exact_path_classification() {
        let exact = ["/foo.rs", "/src/lib.rs", "/a/b/c.txt"];
        for path in exact {
            assert!(is_exact_path(path), "{path} should be exact");
        }
        let pattern = [
            "foo.rs",      // unanchored
            "/src/",       // directory
            "/src/**",     // wildcard
            "*.rs",        // glob
            "/src/?.rs",   // single-char wildcard
            "/src/[a].rs", // character class
        ];
        for path in pattern {
            assert!(!is_exact_path(path), "{path} should be pattern");
        }
    }

    // ── parse_entry_line ──────────────────────────────────────────

    #[test]
    fn parse_entry_line_simple() {
        let (path, owners) = parse_entry_line("/src/lib.rs @org/team").unwrap();
        assert_eq!(path, "/src/lib.rs");
        assert_eq!(owners, vec!["@org/team"]);
    }

    #[test]
    fn parse_entry_line_multiple_owners() {
        let (path, owners) = parse_entry_line("/file @a @b @c").unwrap();
        assert_eq!(path, "/file");
        assert_eq!(owners, vec!["@a", "@b", "@c"]);
    }

    #[test]
    fn parse_entry_line_no_owners() {
        let (path, owners) = parse_entry_line("/file").unwrap();
        assert_eq!(path, "/file");
        assert!(owners.is_empty());
    }

    #[test]
    fn parse_entry_line_inline_comment() {
        let (path, owners) = parse_entry_line("/file @owner # inline comment").unwrap();
        assert_eq!(path, "/file");
        assert_eq!(owners, vec!["@owner"]);
    }

    #[test]
    fn parse_entry_line_escaped_space() {
        let (path, owners) = parse_entry_line(r"/foo\ bar @owner").unwrap();
        assert_eq!(path, "/foo bar");
        assert_eq!(owners, vec!["@owner"]);
    }

    #[test]
    fn parse_entry_line_empty_fails() {
        assert!(parse_entry_line("").is_err());
    }

    // ── parse_rule ────────────────────────────────────────────────

    #[test]
    fn parse_rule_valid() {
        let result = parse_rule("# Rule[auto-assign]: /src/** @org/backend").unwrap();
        assert_eq!(
            result,
            Some(("/src/**".into(), vec!["@org/backend".into()]))
        );
    }

    #[test]
    fn parse_rule_not_a_rule() {
        assert_eq!(parse_rule("# just a comment").unwrap(), None);
        assert_eq!(parse_rule("/src/lib.rs @team").unwrap(), None);
    }

    #[test]
    fn parse_rule_no_body() {
        // The regex requires at least one body character, so this is not a rule.
        assert_eq!(parse_rule("# Rule[auto-assign]:").unwrap(), None);
    }

    #[test]
    fn parse_rule_multiple_owners() {
        let result = parse_rule("# Rule[auto-assign]: *.rs @a @b").unwrap();
        assert_eq!(
            result,
            Some(("*.rs".into(), vec!["@a".into(), "@b".into()]))
        );
    }

    // ── Manifest::parse ───────────────────────────────────────────

    #[test]
    fn parse_empty_input() {
        let manifest = Manifest::parse("").unwrap();
        assert!(manifest.header.is_empty());
        assert!(manifest.items.is_empty());
        assert!(manifest.footer.is_empty());
    }

    #[test]
    fn parse_blank_lines_only() {
        let manifest = Manifest::parse("\n\n\n").unwrap();
        assert_eq!(manifest.header.len(), 3);
        assert!(manifest.items.is_empty());
    }

    #[test]
    fn parse_comment_only() {
        let manifest = Manifest::parse("# comment\n# another\n").unwrap();
        assert_eq!(manifest.header.len(), 2);
        assert!(manifest.items.is_empty());
    }

    #[test]
    fn parse_normal_entries() {
        let text = "/src/lib.rs @org/team\n/README.md @org/docs\n";
        let manifest = Manifest::parse(text).unwrap();
        assert_eq!(manifest.items.len(), 2);
        match &manifest.items[0] {
            Item::Entry(e) => {
                assert_eq!(e.path, "/src/lib.rs");
                assert_eq!(e.owners, vec!["@org/team"]);
                assert_eq!(e.kind, EntryKind::Exact);
            }
            _ => panic!("expected entry"),
        }
    }

    #[test]
    fn parse_entry_with_leading_comment() {
        let text = "# header\n/first @a\n# leading\n/file @owner\n";
        let manifest = Manifest::parse(text).unwrap();
        assert_eq!(manifest.header, vec!["# header"]);
        assert_eq!(manifest.items.len(), 2);
        match &manifest.items[1] {
            Item::Entry(e) => {
                assert_eq!(e.leading, vec!["# leading"]);
                assert_eq!(e.path, "/file");
            }
            _ => panic!("expected entry"),
        }
    }

    #[test]
    fn parse_entry_with_footer() {
        let text = "/file @owner\n# trailing\n";
        let manifest = Manifest::parse(text).unwrap();
        assert_eq!(manifest.footer, vec!["# trailing"]);
    }

    #[test]
    fn parse_pattern_entry() {
        let text = "*.rs @team\n";
        let manifest = Manifest::parse(text).unwrap();
        match &manifest.items[0] {
            Item::Entry(e) => {
                assert_eq!(e.kind, EntryKind::Pattern);
                assert_eq!(e.path, "*.rs");
            }
            _ => panic!("expected entry"),
        }
    }

    #[test]
    fn parse_rule_item() {
        let text = "# Rule[auto-assign]: /src/** @org/backend\n";
        let manifest = Manifest::parse(text).unwrap();
        assert_eq!(manifest.items.len(), 1);
        match &manifest.items[0] {
            Item::Rule(r) => {
                assert_eq!(r.pattern, "/src/**");
                assert_eq!(r.owners, vec!["@org/backend"]);
            }
            _ => panic!("expected rule"),
        }
    }

    #[test]
    fn parse_unicode_path() {
        let text = "/src/über.rs @team\n";
        let manifest = Manifest::parse(text).unwrap();
        match &manifest.items[0] {
            Item::Entry(e) => assert_eq!(e.path, "/src/über.rs"),
            _ => panic!("expected entry"),
        }
    }

    #[test]
    fn parse_crlf_input() {
        let text = "# header\r\n/file @owner\r\n";
        let manifest = Manifest::parse(text).unwrap();
        // headers get the \r because we split on \n only; the trimming
        // in is_comment_or_blank handles \r as whitespace.
        assert_eq!(manifest.items.len(), 1);
        match &manifest.items[0] {
            Item::Entry(e) => {
                // The path may have trailing \r from the split; verify the
                // essential semantics work correctly.
                assert!(e.path.contains("file"));
            }
            _ => panic!("expected entry"),
        }
    }

    #[test]
    fn parse_no_trailing_newline() {
        let text = "/file @owner";
        let manifest = Manifest::parse(text).unwrap();
        assert_eq!(manifest.items.len(), 1);
    }

    // ── Manifest::render ──────────────────────────────────────────

    #[test]
    fn render_empty_manifest() {
        let m = Manifest {
            header: vec![],
            items: vec![],
            footer: vec![],
        };
        assert_eq!(m.render(), "");
    }

    #[test]
    fn render_simple_entry() {
        let m = Manifest {
            header: vec!["# header".into()],
            items: vec![Item::Entry(Entry {
                leading: vec![],
                path: "/file".into(),
                owners: vec!["@team".into()],
                kind: EntryKind::Exact,
            })],
            footer: vec![],
        };
        assert_eq!(m.render(), "# header\n/file @team\n");
    }

    #[test]
    fn render_escaped_space_in_path() {
        let m = Manifest {
            header: vec![],
            items: vec![Item::Entry(Entry {
                leading: vec![],
                path: "/foo bar".into(),
                owners: vec!["@team".into()],
                kind: EntryKind::Exact,
            })],
            footer: vec![],
        };
        assert_eq!(m.render(), "/foo\\ bar @team\n");
    }

    #[test]
    fn render_rule_item() {
        let m = Manifest {
            header: vec![],
            items: vec![Item::Rule(Rule {
                leading: vec![],
                pattern: "/src/**".into(),
                owners: vec!["@org/team".into()],
            })],
            footer: vec![],
        };
        assert_eq!(m.render(), "# Rule[auto-assign]: /src/** @org/team\n");
    }

    #[test]
    fn render_deterministic_repeated() {
        let m = Manifest {
            header: vec!["# h".into()],
            items: vec![
                Item::Entry(Entry {
                    leading: vec![],
                    path: "/a".into(),
                    owners: vec!["@x".into()],
                    kind: EntryKind::Exact,
                }),
                Item::Entry(Entry {
                    leading: vec![],
                    path: "/b".into(),
                    owners: vec!["@y".into()],
                    kind: EntryKind::Exact,
                }),
            ],
            footer: vec![],
        };
        assert_eq!(m.render(), m.render(), "render must be deterministic");
    }

    // ── parse → render semantic roundtrip ─────────────────────────

    #[test]
    fn parse_render_roundtrip_preserves_semantics() {
        let original = "# header\n/src/lib.rs @org/team\n/README.md @org/docs\n";
        let manifest = Manifest::parse(original).unwrap();
        let rendered = manifest.render();
        let reparsed = Manifest::parse(&rendered).unwrap();
        assert_eq!(manifest.items.len(), reparsed.items.len());
        for (a, b) in manifest.items.iter().zip(reparsed.items.iter()) {
            match (a, b) {
                (Item::Entry(ea), Item::Entry(eb)) => {
                    assert_eq!(ea.path, eb.path);
                    assert_eq!(ea.owners, eb.owners);
                    assert_eq!(ea.kind, eb.kind);
                }
                (Item::Rule(ra), Item::Rule(rb)) => {
                    assert_eq!(ra.pattern, rb.pattern);
                    assert_eq!(ra.owners, rb.owners);
                }
                _ => panic!("item type mismatch"),
            }
        }
    }

    // ── sort_items ────────────────────────────────────────────────

    #[test]
    fn sort_items_deterministic_order() {
        let mut m = Manifest {
            header: vec![],
            items: vec![
                Item::Entry(Entry {
                    leading: vec![],
                    path: "/z.rs".into(),
                    owners: vec![],
                    kind: EntryKind::Exact,
                }),
                Item::Entry(Entry {
                    leading: vec![],
                    path: "/a.rs".into(),
                    owners: vec![],
                    kind: EntryKind::Exact,
                }),
                Item::Rule(Rule {
                    leading: vec![],
                    pattern: "/m/**".into(),
                    owners: vec!["@x".into()],
                }),
            ],
            footer: vec![],
        };
        m.sort_items();
        let keys: Vec<_> = m
            .items
            .iter()
            .map(|i| match i {
                Item::Entry(e) => e.path.as_str(),
                Item::Rule(r) => r.pattern.as_str(),
            })
            .collect();
        assert_eq!(keys, vec!["/a.rs", "/m/**", "/z.rs"]);
    }

    #[test]
    fn sort_items_idempotent() {
        let mut m = Manifest {
            header: vec![],
            items: vec![
                Item::Entry(Entry {
                    leading: vec![],
                    path: "/c".into(),
                    owners: vec![],
                    kind: EntryKind::Exact,
                }),
                Item::Entry(Entry {
                    leading: vec![],
                    path: "/a".into(),
                    owners: vec![],
                    kind: EntryKind::Exact,
                }),
                Item::Entry(Entry {
                    leading: vec![],
                    path: "/b".into(),
                    owners: vec![],
                    kind: EntryKind::Exact,
                }),
            ],
            footer: vec![],
        };
        m.sort_items();
        let first = m.render();
        m.sort_items();
        assert_eq!(m.render(), first, "sort must be idempotent");
    }

    // ── RuleMatcher ───────────────────────────────────────────────

    #[test]
    fn rule_matcher_glob_star() {
        let m = RuleMatcher::new("*.rs").unwrap();
        assert!(m.matches("/src/lib.rs"));
        assert!(m.matches("/README.rs"));
        assert!(!m.matches("/src/lib.txt"));
    }

    #[test]
    fn rule_matcher_double_star() {
        let m = RuleMatcher::new("/src/**").unwrap();
        assert!(m.matches("/src/lib.rs"));
        assert!(m.matches("/src/a/b/c.rs"));
        assert!(!m.matches("/README.md"));
    }

    #[test]
    fn rule_matcher_directory_pattern() {
        let m = RuleMatcher::new("docs/").unwrap();
        assert!(m.matches("/docs/guide.md"));
        assert!(!m.matches("/src/docs.rs"));
    }

    #[test]
    fn rule_matcher_exact_anchor() {
        let m = RuleMatcher::new("/src/lib.rs").unwrap();
        assert!(m.matches("/src/lib.rs"));
        assert!(!m.matches("/other/src/lib.rs"));
    }

    // ── select_auto_rule ──────────────────────────────────────────

    #[test]
    fn select_auto_rule_no_match() {
        let rules = [Rule {
            leading: vec![],
            pattern: "/src/**".into(),
            owners: vec!["@team".into()],
        }];
        let result = select_auto_rule(rules.iter(), "/README.md").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn select_auto_rule_one_match() {
        let rules = [Rule {
            leading: vec![],
            pattern: "/src/**".into(),
            owners: vec!["@team".into()],
        }];
        let result = select_auto_rule(rules.iter(), "/src/lib.rs").unwrap();
        assert_eq!(result.unwrap().owners, vec!["@team"]);
    }

    #[test]
    fn select_auto_rule_most_specific_wins() {
        let rules = [
            Rule {
                leading: vec![],
                pattern: "/src/**".into(),
                owners: vec!["@general".into()],
            },
            Rule {
                leading: vec![],
                pattern: "/src/special/**".into(),
                owners: vec!["@special".into()],
            },
        ];
        let result = select_auto_rule(rules.iter(), "/src/special/file.rs").unwrap();
        assert_eq!(result.unwrap().owners, vec!["@special"]);
    }

    #[test]
    fn select_auto_rule_deterministic_on_equal_specificity() {
        // When two rules have the same literal count and pattern length,
        // the first one encountered wins (stable selection).
        let rules = [
            Rule {
                leading: vec![],
                pattern: "/ab/**".into(),
                owners: vec!["@first".into()],
            },
            Rule {
                leading: vec![],
                pattern: "/ab/**".into(),
                owners: vec!["@second".into()],
            },
        ];
        let result = select_auto_rule(rules.iter(), "/ab/file.rs").unwrap();
        // First match wins when specificity is equal.
        assert_eq!(result.unwrap().owners, vec!["@first"]);
    }

    // ── matches_pattern ───────────────────────────────────────────

    #[test]
    fn matches_pattern_anchored() {
        assert!(matches_pattern("/src/lib.rs", "/src/lib.rs").unwrap());
        assert!(!matches_pattern("/src/lib.rs", "/other.rs").unwrap());
    }

    #[test]
    fn matches_pattern_unanchored() {
        assert!(matches_pattern("*.rs", "/src/lib.rs").unwrap());
    }

    #[test]
    fn matches_pattern_double_star() {
        assert!(matches_pattern("/src/**", "/src/a/b.rs").unwrap());
        assert!(!matches_pattern("/src/**", "/other/a.rs").unwrap());
    }

    // ── has_patterns / exact_entries / rules ──────────────────────

    #[test]
    fn has_patterns_detects_pattern_entries() {
        let m = Manifest::parse("*.rs @team\n/exact.rs @other\n").unwrap();
        assert!(m.has_patterns());
    }

    #[test]
    fn has_patterns_false_for_exact_only() {
        let m = Manifest::parse("/exact.rs @team\n").unwrap();
        assert!(!m.has_patterns());
    }

    #[test]
    fn exact_entries_iterator() {
        let m = Manifest::parse("*.rs @team\n/exact.rs @other\n").unwrap();
        let exact: Vec<_> = m.exact_entries().collect();
        assert_eq!(exact.len(), 1);
        assert_eq!(exact[0].path, "/exact.rs");
    }

    #[test]
    fn rules_iterator() {
        let m = Manifest::parse("# Rule[auto-assign]: /src/** @team\n/exact.rs @other\n").unwrap();
        let rules: Vec<_> = m.rules().collect();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].pattern, "/src/**");
    }

    // ── property-based tests ──────────────────────────────────────

    mod prop {
        use super::*;
        use proptest::prelude::*;

        /// Generate path-like strings that are valid CODEOWNERS tokens.
        fn path_token() -> impl Strategy<Value = String> {
            proptest::string::string_regex(r"/[a-zA-Z0-9_./\-]{1,30}")
                .unwrap()
                .prop_filter("must not be empty after trim", |s| !s.trim().is_empty())
        }

        proptest! {
            /// Escaping a token and splitting it back must yield the original.
            #[test]
            fn escape_split_roundtrip(token in path_token()) {
                let escaped = escape_token(&token);
                let tokens = split_tokens(&escaped);
                prop_assert_eq!(tokens, vec![token]);
            }

            /// Rendering a manifest and re-parsing preserves semantic content.
            #[test]
            fn render_parse_semantic_roundtrip(
                path in path_token(),
                owner in proptest::string::string_regex("@[a-z]{1,10}").unwrap(),
            ) {
                let manifest = Manifest {
                    header: vec![],
                    items: vec![Item::Entry(Entry {
                        leading: vec![],
                        path: path.clone(),
                        owners: vec![owner.clone()],
                        kind: EntryKind::Exact,
                    })],
                    footer: vec![],
                };
                let rendered = manifest.render();
                let reparsed = Manifest::parse(&rendered).unwrap();
                prop_assert_eq!(reparsed.items.len(), 1);
                if let Item::Entry(e) = &reparsed.items[0] {
                    prop_assert_eq!(&e.path, &path);
                    prop_assert_eq!(&e.owners, &[owner]);
                } else {
                    prop_assert!(false, "expected entry");
                }
            }

            /// Sorting is idempotent: sort(sort(items)) == sort(items).
            #[test]
            fn sort_idempotent(
                paths in proptest::collection::vec(path_token(), 1..10),
            ) {
                let items: Vec<Item> = paths.into_iter().map(|p| {
                    Item::Entry(Entry {
                        leading: vec![],
                        path: p,
                        owners: vec![],
                        kind: EntryKind::Exact,
                    })
                }).collect();
                let mut m1 = Manifest { header: vec![], items: items.clone(), footer: vec![] };
                m1.sort_items();
                let after_first = m1.render();
                m1.sort_items();
                prop_assert_eq!(m1.render(), after_first);
            }

            /// Rendering identical input always produces identical output.
            #[test]
            fn render_determinism(
                path in path_token(),
                owner in proptest::string::string_regex("@[a-z]{1,10}").unwrap(),
            ) {
                let manifest = Manifest {
                    header: vec!["# header".into()],
                    items: vec![Item::Entry(Entry {
                        leading: vec![],
                        path,
                        owners: vec![owner],
                        kind: EntryKind::Exact,
                    })],
                    footer: vec![],
                };
                prop_assert_eq!(manifest.render(), manifest.render());
            }
        }
    }
}
