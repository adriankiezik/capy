use crate::error::{Result, StyleError};
use proc_macro2::{LineColumn, Span};
use std::ops::Range;

pub struct Source<'a> {
    text: &'a str,
    lines: Vec<usize>,
}

impl<'a> Source<'a> {
    pub fn new(text: &'a str) -> Self {
        let lines = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(offset, _)| offset + 1))
            .collect();

        Self { text, lines }
    }

    fn offset(&self, position: LineColumn) -> Result<usize> {
        let start = *self
            .lines
            .get(position.line.saturating_sub(1))
            .ok_or(StyleError::SourceLine)?;

        let line = self.text[start..]
            .split('\n')
            .next()
            .ok_or(StyleError::MissingSourceLine)?;

        let column = line
            .char_indices()
            .map(|(offset, _)| offset)
            .chain(std::iter::once(line.len()))
            .nth(position.column)
            .ok_or(StyleError::SourceColumn)?;

        Ok(start + column)
    }

    pub fn range(&self, span: Span) -> Result<Range<usize>> {
        Ok(self.offset(span.start())?..self.offset(span.end())?)
    }
}

pub struct Edit {
    pub range: Range<usize>,
    pub replacement: String,
}

pub fn apply(text: &str, mut edits: Vec<Edit>) -> Result<String> {
    edits.sort_by_key(|edit| (edit.range.start, edit.range.end));

    let mut output = String::new();

    let mut end = 0;

    for edit in edits {
        if edit.range.start < end || edit.range.end < edit.range.start {
            return Err(StyleError::OverlappingEdits);
        }

        output.push_str(
            text.get(end..edit.range.start)
                .ok_or(StyleError::Boundary("edit"))?,
        );

        output.push_str(&edit.replacement);

        end = edit.range.end;
    }

    output.push_str(text.get(end..).ok_or(StyleError::Boundary("final edit"))?);

    Ok(output)
}

pub fn spacing(text: &str, range: Range<usize>, indentation: usize) -> Result<Option<Edit>> {
    let gap = text
        .get(range.clone())
        .ok_or(StyleError::Boundary("spacing"))?;

    if !gap.trim().is_empty() || gap.bytes().filter(|&byte| byte == b'\n').count() >= 2 {
        return Ok(None);
    }

    let indent = gap
        .rsplit_once('\n')
        .map_or_else(|| " ".repeat(indentation), |(_, tail)| tail.to_owned());

    Ok(Some(Edit {
        range,
        replacement: format!("\n\n{indent}"),
    }))
}

pub fn group_spacing(text: &str, range: Range<usize>, indentation: usize) -> Result<Option<Edit>> {
    let gap = text
        .get(range.clone())
        .ok_or(StyleError::Boundary("assignment spacing"))?;

    if !gap.trim().is_empty() {
        return Ok(None);
    }

    let indent = gap
        .rsplit_once('\n')
        .map_or_else(|| " ".repeat(indentation), |(_, tail)| tail.to_owned());

    let replacement = format!("\n{indent}");

    Ok((gap != replacement).then_some(Edit { range, replacement }))
}

pub fn strip_comments(text: &str, protected: &[Range<usize>]) -> Result<String> {
    let bytes = text.as_bytes();

    let mut edits = Vec::new();

    let mut cursor = 0;

    let mut literals = protected.iter().peekable();

    while cursor < bytes.len() {
        while literals.peek().is_some_and(|range| range.end <= cursor) {
            literals.next();
        }

        if let Some(range) = literals.peek()
            && range.start <= cursor
            && cursor < range.end
        {
            cursor = range.end;

            continue;
        }

        let start = cursor;

        if bytes.get(cursor..cursor + 2) == Some(b"//") {
            while cursor < bytes.len() && bytes[cursor] != b'\n' {
                cursor += 1;
            }

            edits.push(Edit {
                range: start..cursor,
                replacement: String::new(),
            });
        } else if bytes.get(cursor..cursor + 2) == Some(b"/*") {
            cursor += 2;

            let mut depth = 1;

            while cursor < bytes.len() && depth > 0 {
                match bytes.get(cursor..cursor + 2) {
                    Some(b"/*") => {
                        depth += 1;
                        cursor += 2;
                    }
                    Some(b"*/") => {
                        depth -= 1;
                        cursor += 2;
                    }
                    _ => cursor += 1,
                }
            }

            if depth != 0 {
                return Err(StyleError::BlockComment(start));
            }

            let newlines = text[start..cursor]
                .bytes()
                .filter(|&byte| byte == b'\n')
                .count();

            let replacement = if newlines == 0 {
                " ".to_owned()
            } else {
                "\n".repeat(newlines)
            };

            edits.push(Edit {
                range: start..cursor,
                replacement,
            });
        } else {
            cursor += 1;
        }
    }

    apply(text, edits)
}
