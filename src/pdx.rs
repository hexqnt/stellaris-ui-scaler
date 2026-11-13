use std::{borrow::Cow, ops::Range};

use anyhow::{Result, bail, ensure};

#[derive(Debug, Clone)]
pub enum Value<'a> {
    Atom { text: &'a str, span: Range<usize> },
    Block { children: Vec<Node<'a>>, end: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Atom,
    Open,
    Close,
    Operator,
}

#[derive(Debug, Clone)]
pub struct Node<'a> {
    pub key: Option<&'a str>,
    pub span: Range<usize>,
    pub value: Value<'a>,
}

impl<'a> Node<'a> {
    #[must_use]
    pub fn children(&self) -> &[Self] {
        match &self.value {
            Value::Block { children, .. } => children,
            Value::Atom { .. } => &[],
        }
    }

    #[must_use]
    pub fn field(&self, key: &str) -> Option<&Self> {
        self.children()
            .iter()
            .find(|n| n.key.is_some_and(|k| k.eq_ignore_ascii_case(key)))
    }

    #[must_use]
    pub fn atom(&self) -> Option<&'a str> {
        match &self.value {
            Value::Atom { text, .. } => Some(
                text.strip_prefix('"')
                    .and_then(|value| value.strip_suffix('"'))
                    .unwrap_or(text),
            ),
            Value::Block { .. } => None,
        }
    }

    pub fn named(&self) -> Option<&'a str> {
        self.field("name").and_then(Self::atom)
    }

    #[must_use]
    pub fn named_child(&self, name: &str) -> Option<&Self> {
        self.children()
            .iter()
            .find(|child| child.named() == Some(name))
    }

    #[must_use]
    pub fn parse_atom<T: std::str::FromStr>(&self) -> Option<T> {
        self.atom()?.parse().ok()
    }

    pub fn walk<'doc>(&'doc self, visit: &mut impl FnMut(&'doc Self)) {
        self.try_walk(&mut |node| {
            visit(node);
            Ok::<_, std::convert::Infallible>(())
        })
        .unwrap_or_else(|never| match never {});
    }

    /// Обходит поддерево, включая сам узел, до первой ошибки обработчика.
    /// # Errors
    /// Возвращает ошибку обработчика без посещения оставшихся узлов.
    pub fn try_walk<'doc, E>(
        &'doc self,
        visit: &mut impl FnMut(&'doc Self) -> std::result::Result<(), E>,
    ) -> std::result::Result<(), E> {
        visit(self)?;
        for child in self.children() {
            child.try_walk(visit)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Document<'a> {
    pub source: &'a str,
    pub nodes: Vec<Node<'a>>,
}

impl<'a> Document<'a> {
    /// # Errors
    /// Возвращает ошибку с позицией незакрытой конструкции или оператора без ключа либо значения.
    pub fn parse(source: &'a str) -> Result<Self> {
        let bytes = source.as_bytes();
        let mut tokens = Vec::new();
        let mut i = usize::from(source.starts_with('\u{feff}')) * 3;
        while let Some(&c) = bytes.get(i) {
            if c.is_ascii_whitespace() {
                i += 1;
                continue;
            }
            if c == b'#' {
                while bytes.get(i).is_some_and(|c| *c != b'\n') {
                    i += 1;
                }
                continue;
            }
            let start = i;
            i += 1;
            let kind = match c {
                b'"' => {
                    let mut closed = false;
                    while let Some(&c) = bytes.get(i) {
                        i += 1;
                        if c == b'\\' && matches!(bytes.get(i), Some(b'"' | b'\\')) {
                            i += 1;
                        } else if c == b'"' {
                            closed = true;
                            break;
                        }
                    }
                    ensure!(closed, "Unterminated string at byte {start}");
                    TokenKind::Atom
                }
                b'@' if bytes.get(i) == Some(&b'[') => {
                    i += 1;
                    while bytes.get(i).is_some_and(|c| *c != b']') {
                        i += 1;
                    }
                    ensure!(
                        bytes.get(i) == Some(&b']'),
                        "Unterminated expression at byte {start}"
                    );
                    i += 1;
                    TokenKind::Atom
                }
                b'{' => TokenKind::Open,
                b'}' => TokenKind::Close,
                b'=' => TokenKind::Operator,
                b'<' | b'>' | b'!' => {
                    if bytes.get(i) == Some(&b'=') {
                        i += 1;
                    }
                    TokenKind::Operator
                }
                _ => {
                    while bytes
                        .get(i)
                        .is_some_and(|c| !c.is_ascii_whitespace() && !b"{}=#<>!".contains(c))
                    {
                        i += 1;
                    }
                    TokenKind::Atom
                }
            };
            tokens.push(Token {
                kind,
                text: &source[start..i],
                start,
            });
        }
        let mut cursor = 0;
        let nodes = parse_nodes(&tokens, &mut cursor, 0)?;
        Ok(Self { source, nodes })
    }

    pub fn walk<'doc>(&'doc self, mut visit: impl FnMut(&'doc Node<'a>)) {
        for node in &self.nodes {
            node.walk(&mut visit);
        }
    }

    /// Обходит узлы в глубину и останавливается при первой ошибке обработчика.
    /// # Errors
    /// Возвращает ошибку обработчика без посещения оставшихся узлов.
    pub fn try_walk<'doc, E>(
        &'doc self,
        mut visit: impl FnMut(&'doc Node<'a>) -> std::result::Result<(), E>,
    ) -> std::result::Result<(), E> {
        for node in &self.nodes {
            node.try_walk(&mut visit)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct Token<'a> {
    kind: TokenKind,
    text: &'a str,
    start: usize,
}

impl Token<'_> {
    // Длину уже хранит срез; дополнительный конец диапазона увеличил бы каждый токен.
    const fn end(&self) -> usize {
        self.start + self.text.len()
    }
}

#[derive(Default)]
pub struct Edits(Vec<(Range<usize>, String)>);

impl Edits {
    pub fn replace(&mut self, span: Range<usize>, text: impl Into<String>) {
        self.0.push((span, text.into()));
    }

    pub fn atom(&mut self, node: &Node<'_>, text: impl Into<String>) {
        if let Value::Atom { span, .. } = &node.value {
            self.replace(span.clone(), text);
        }
    }

    pub fn insert(&mut self, node: &Node<'_>, text: &str) {
        if let Value::Block { end, .. } = node.value {
            self.replace(end..end, text);
        }
    }

    /// # Errors
    /// Возвращает ошибку пересечения диапазонов или нарушения границ UTF-8.
    pub fn apply(mut self, source: &str) -> Result<String> {
        self.0.sort_by_key(|(r, _)| (r.start, r.end));
        let mut capacity = source.len();
        let mut cursor = 0;
        for (range, replacement) in &self.0 {
            ensure!(
                range.start >= cursor && range.start <= range.end && range.end <= source.len(),
                "Overlapping or invalid source edits"
            );
            ensure!(
                source.is_char_boundary(range.start) && source.is_char_boundary(range.end),
                "Invalid UTF-8 edit boundary"
            );
            capacity = (capacity - range.len())
                .checked_add(replacement.len())
                .ok_or_else(|| anyhow::anyhow!("Edited source length overflow"))?;
            cursor = range.end;
        }
        let mut out = String::with_capacity(capacity);
        let mut cursor = 0;
        for (range, replacement) in self.0 {
            out.push_str(&source[cursor..range.start]);
            out.push_str(&replacement);
            cursor = range.end;
        }
        out.push_str(&source[cursor..]);
        Ok(out)
    }
}
/// Декодирует только экранированные кавычки и обратные слеши, сохраняя остальные последовательности.
pub(crate) fn unescape(text: &str) -> Cow<'_, str> {
    let Some(start) = text
        .as_bytes()
        .windows(2)
        .position(|pair| matches!(pair, [b'\\', b'\\' | b'"']))
    else {
        return Cow::Borrowed(text);
    };
    let mut decoded = String::with_capacity(text.len());
    decoded.push_str(&text[..start]);
    let mut chars = text[start..].chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\'
            && let Some(escaped) = chars.next_if(|c| matches!(c, '\\' | '"'))
        {
            decoded.push(escaped);
        } else {
            decoded.push(c);
        }
    }
    Cow::Owned(decoded)
}

fn parse_nodes<'a>(
    tokens: &[Token<'a>],
    cursor: &mut usize,
    depth: usize,
) -> Result<Vec<Node<'a>>> {
    ensure!(depth <= 256, "Maximum syntax nesting depth exceeded");
    let mut nodes = Vec::new();
    while let Some(first) = tokens.get(*cursor) {
        if first.kind == TokenKind::Close {
            ensure!(
                depth > 0,
                "Unexpected closing brace at byte {}",
                first.start
            );
            return Ok(nodes);
        }
        let start = first.start;
        ensure!(
            first.kind != TokenKind::Operator,
            "Unexpected operator at byte {start}"
        );
        let mut key = None;
        if first.kind == TokenKind::Atom
            && let Some(next) = tokens.get(*cursor + 1)
            && matches!(next.kind, TokenKind::Operator | TokenKind::Open)
        {
            key = Some(first.text);
            *cursor += if next.kind == TokenKind::Open { 1 } else { 2 };
        }
        let token = tokens
            .get(*cursor)
            .ok_or_else(|| anyhow::anyhow!("Missing value at byte {start}"))?;
        ensure!(
            matches!(token.kind, TokenKind::Atom | TokenKind::Open),
            "Missing value at byte {start}"
        );
        *cursor += 1;
        let (value, end) = if token.kind == TokenKind::Open {
            let children = parse_nodes(tokens, cursor, depth + 1)?;
            let Some(close) = tokens.get(*cursor) else {
                bail!("Unclosed block at byte {start}");
            };
            *cursor += 1;
            (
                Value::Block {
                    children,
                    end: close.start,
                },
                close.end(),
            )
        } else {
            (
                Value::Atom {
                    text: token.text,
                    span: token.start..token.end(),
                },
                token.end(),
            )
        };
        nodes.push(Node {
            key,
            span: start..end,
            value,
        });
    }
    ensure!(depth == 0, "Unclosed block at end of file");
    Ok(nodes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_text_decodes_only_supported_escapes() {
        for (source, expected) in [
            (r"D:\\Mods\\UI", r"D:\Mods\UI"),
            (r#"mod/a\"b"#, "mod/a\"b"),
            (r"mod/UI\\", r"mod/UI\"),
            (r#"Текст\\\""#, "Текст\\\""),
        ] {
            assert_eq!(unescape(source), expected);
        }
        for text in ["mod/ui", r"D:\Mods\UI", r"mod/UI\", "Текст"] {
            assert!(matches!(unescape(text), Cow::Borrowed(value) if value == text));
        }
    }
}
