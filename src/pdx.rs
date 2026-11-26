use std::{borrow::Cow, iter::Peekable, ops::Range};

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
            Value::Atom { text, .. } => Some(text.strip_circumfix('"', '"').unwrap_or(text)),
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
            Ok::<_, !>(())
        });
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
        let mut cursor = usize::from(source.starts_with('\u{feff}')) * 3;
        let mut tokens = std::iter::from_fn(|| {
            let token = lex(source, &mut cursor);
            if token.is_err() {
                cursor = source.len();
            }
            token.transpose()
        })
        .peekable();
        match parse_nodes(&mut tokens, 0) {
            Ok((nodes, _)) => Ok(Self { source, nodes }),
            Err(error) => {
                // Как при отдельном проходе лексера, ошибка токена важнее ошибки структуры.
                for token in tokens {
                    token?;
                }
                Err(error)
            }
        }
    }

    pub fn walk<'doc>(&'doc self, mut visit: impl FnMut(&'doc Node<'a>)) {
        self.try_walk(|node| {
            visit(node);
            Ok::<_, !>(())
        });
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

fn lex<'a>(source: &'a str, i: &mut usize) -> Result<Option<Token<'a>>> {
    let bytes = source.as_bytes();
    let (start, first) = loop {
        let Some(&first) = bytes.get(*i) else {
            return Ok(None);
        };
        let start = *i;
        *i += 1;
        if first == b'#' {
            while bytes.get(*i).is_some_and(|c| *c != b'\n') {
                *i += 1;
            }
        } else if !first.is_ascii_whitespace() {
            break (start, first);
        }
    };
    let kind = match first {
        b'"' => {
            let len = quoted_end(&bytes[*i..])
                .ok_or_else(|| anyhow::anyhow!("Unterminated string at byte {start}"))?;
            *i += len + 1;
            TokenKind::Atom
        }
        b'@' if bytes.get(*i) == Some(&b'[') => {
            *i += 1;
            while bytes.get(*i).is_some_and(|c| *c != b']') {
                *i += 1;
            }
            ensure!(
                bytes.get(*i) == Some(&b']'),
                "Unterminated expression at byte {start}"
            );
            *i += 1;
            TokenKind::Atom
        }
        b'{' => TokenKind::Open,
        b'}' => TokenKind::Close,
        b'=' => TokenKind::Operator,
        b'<' | b'>' | b'!' => {
            *i += usize::from(bytes.get(*i) == Some(&b'='));
            TokenKind::Operator
        }
        _ => {
            while bytes
                .get(*i)
                .is_some_and(|c| !c.is_ascii_whitespace() && !b"{}=#<>!".contains(c))
            {
                *i += 1;
            }
            TokenKind::Atom
        }
    };
    Ok(Some(Token {
        kind,
        text: &source[start..*i],
        start,
    }))
}

/// Ищет закрывающую кавычку в срезе после открывающей.
pub(crate) fn quoted_end(bytes: &[u8]) -> Option<usize> {
    let mut end = 0;
    while let Some(&byte) = bytes.get(end) {
        if byte == b'"' {
            return Some(end);
        }
        end += if byte == b'\\' && matches!(bytes.get(end + 1), Some(b'\\' | b'"')) {
            2
        } else {
            1
        };
    }
    None
}

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
        self.insert_text(node, text);
    }

    pub(crate) fn insert_text(&mut self, node: &Node<'_>, text: impl Into<String>) {
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
        .array_windows::<2>()
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
    tokens: &mut Peekable<impl Iterator<Item = Result<Token<'a>>>>,
    depth: usize,
) -> Result<(Vec<Node<'a>>, usize)> {
    ensure!(depth <= 256, "Maximum syntax nesting depth exceeded");
    let mut nodes = Vec::new();
    while let Some(first) = tokens.next().transpose()? {
        let start = first.start;
        if first.kind == TokenKind::Close {
            ensure!(depth > 0, "Unexpected closing brace at byte {start}");
            return Ok((nodes, start));
        }
        ensure!(
            first.kind != TokenKind::Operator,
            "Unexpected operator at byte {start}"
        );
        let mut key = None;
        let token = if first.kind == TokenKind::Atom
            && let Some(Ok(next)) = tokens.peek()
            && matches!(next.kind, TokenKind::Operator | TokenKind::Open)
        {
            key = Some(first.text);
            if next.kind == TokenKind::Operator {
                tokens.next();
            }
            tokens
                .next()
                .transpose()?
                .ok_or_else(|| anyhow::anyhow!("Missing value at byte {start}"))?
        } else {
            first
        };
        let (value, end) = match token.kind {
            TokenKind::Open => {
                let (children, end) = parse_nodes(tokens, depth + 1)?;
                (Value::Block { children, end }, end + 1)
            }
            TokenKind::Atom => (
                Value::Atom {
                    text: token.text,
                    span: token.start..token.end(),
                },
                token.end(),
            ),
            TokenKind::Close | TokenKind::Operator => bail!("Missing value at byte {start}"),
        };
        nodes.push(Node {
            key,
            span: start..end,
            value,
        });
    }
    ensure!(depth == 0, "Unclosed block at end of file");
    Ok((nodes, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailing_lexical_errors_keep_their_priority() {
        for (source, expected) in [
            ("} name = \"broken", "Unterminated string at byte 9"),
            ("= x @[(x)", "Unterminated expression at byte 4"),
        ] {
            assert_eq!(Document::parse(source).unwrap_err().to_string(), expected);
        }
    }

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
            std::assert_matches!(unescape(text), Cow::Borrowed(value) if value == text);
        }
    }
}
