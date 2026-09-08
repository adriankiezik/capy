use crate::error::{Result, StyleError};
use crate::source::{apply, group_spacing, spacing, strip_comments};
use std::ops::Range;

fn tokens(text: &str) -> Result<Vec<Range<usize>>> {
    let bytes = text.as_bytes();

    let mut cursor = 0;

    let mut ranges = Vec::new();

    while cursor < bytes.len() {
        let start = cursor;

        if bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        } else if bytes.get(cursor..cursor + 2) == Some(b"//") {
            while cursor < bytes.len() && bytes[cursor] != b'\n' {
                cursor += 1;
            }
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
                return Err(StyleError::WgslBlockComment(start));
            }
        } else if bytes[cursor] == b'"' {
            cursor += 1;

            let mut closed = false;

            while cursor < bytes.len() {
                match bytes[cursor] {
                    b'\\' => cursor = (cursor + 2).min(bytes.len()),
                    b'"' => {
                        cursor += 1;
                        closed = true;

                        break;
                    }
                    _ => cursor += 1,
                }
            }

            if !closed {
                return Err(StyleError::QuotedToken(start));
            }

            ranges.push(start..cursor);
        } else if bytes[cursor].is_ascii_alphanumeric()
            || bytes[cursor] == b'_'
            || bytes[cursor] >= 128
        {
            while cursor < bytes.len()
                && (bytes[cursor].is_ascii_alphanumeric()
                    || bytes[cursor] == b'_'
                    || bytes[cursor] >= 128)
            {
                cursor += 1;
            }

            ranges.push(start..cursor);
        } else {
            cursor += 1;

            ranges.push(start..cursor);
        }
    }

    Ok(ranges)
}

fn assignment(text: &str, tokens: &[Range<usize>]) -> bool {
    let Some(first) = tokens.first() else {
        return false;
    };

    if matches!(
        &text[first.clone()],
        "let" | "var" | "const" | "override" | "return" | "if" | "for" | "while" | "switch"
    ) {
        return false;
    }

    tokens.iter().enumerate().any(|(index, token)| {
        if &text[token.clone()] != "=" {
            return false;
        }

        let previous = index
            .checked_sub(1)
            .and_then(|index| tokens.get(index))
            .map(|range| &text[range.clone()]);

        let next = tokens.get(index + 1).map(|range| &text[range.clone()]);

        let shift = index
            .checked_sub(2)
            .and_then(|index| tokens.get(index))
            .is_some_and(|range| Some(&text[range.clone()]) == previous);

        next != Some("=")
            && !matches!(previous, Some("=" | "!"))
            && (!matches!(previous, Some("<" | ">")) || shift)
    })
}

pub fn normalize(text: &str) -> Result<String> {
    let protected = tokens(text)?
        .into_iter()
        .filter(|range| text[range.clone()].starts_with('"'))
        .collect::<Vec<_>>();

    let text = strip_comments(text, &protected)?;

    let tokens = tokens(&text)?;

    let mut parentheses = 0usize;

    let mut braces = 0usize;

    let mut edits = Vec::new();

    let mut statement_start = 0;

    for (index, pair) in tokens.windows(2).enumerate() {
        let current = &text[pair[0].clone()];

        let next = &text[pair[1].clone()];

        match current {
            "(" => parentheses += 1,
            ")" => parentheses = parentheses.saturating_sub(1),
            "{" => {
                braces += 1;
                statement_start = index + 1;
            }
            "}" => {
                braces = braces.saturating_sub(1);
                statement_start = index + 1;
            }
            _ => {}
        }

        let boundary = parentheses == 0
            && matches!(current, ";" | "}")
            && !matches!(next, "}" | ")" | ";" | "," | "else");

        if boundary {
            let end = tokens[index + 1..]
                .iter()
                .position(|range| matches!(&text[range.clone()], ";" | "{"))
                .map_or(tokens.len(), |offset| index + 1 + offset);

            let grouped = current == ";"
                && assignment(&text, &tokens[statement_start..index])
                && assignment(&text, &tokens[index + 1..end]);

            let range = pair[0].end..pair[1].start;

            let edit = if grouped {
                group_spacing(&text, range, braces * 4)?
            } else {
                spacing(&text, range, braces * 4)?
            };

            if let Some(edit) = edit {
                edits.push(edit);
            }
        }

        if current == ";" && parentheses == 0 {
            statement_start = index + 1;
        }
    }

    let mut text = apply(&text, edits)?;

    if !text.ends_with('\n') {
        text.push('\n');
    }

    Ok(text)
}
