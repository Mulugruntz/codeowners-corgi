use std::sync::LazyLock;

use ignore::gitignore::GitignoreBuilder;
use regex::Regex;

use crate::error::{CorgiError, Result};

static RULE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*#\s*Rule\[auto-assign\]:\s*(.+?)\s*$").expect("valid regex"));

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
    pub pattern: String,
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
            pattern: rule.pattern.clone(),
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
    !path.ends_with('/')
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

fn item_sort_key(item: &Item) -> (String, u8, String) {
    match item {
        Item::Rule(rule) => ("".to_string(), 0, rule.pattern.clone()),
        Item::Entry(entry) => ("~".to_string(), 1, entry.path.clone()),
    }
}
