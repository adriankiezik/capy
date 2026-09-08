use crate::error::{Result, StyleError};
use crate::source::{Edit, Source, apply, group_spacing, spacing, strip_comments};
use proc_macro2::{Span, TokenStream, TokenTree};
use std::io::Write;
use std::ops::Range;
use std::process::{Command, Stdio};
use syn::{
    Item,
    spanned::Spanned,
    visit::{self, Visit},
};

fn literals(
    tokens: TokenStream,
    source: &Source<'_>,
    text: &str,
    ranges: &mut Vec<Range<usize>>,
) -> Result<()> {
    for token in tokens {
        match token {
            TokenTree::Group(group) => literals(group.stream(), source, text, ranges)?,
            TokenTree::Literal(literal) => {
                let range = source.range(literal.span())?;

                let spelling = text.get(range.clone()).ok_or(StyleError::LiteralSpan)?;

                if !spelling.starts_with("//") && !spelling.starts_with("/*") {
                    ranges.push(range);
                }
            }
            _ => {}
        }
    }

    Ok(())
}

fn uncomment(text: &str) -> Result<String> {
    let text = match text.strip_prefix('\u{feff}') {
        Some(text) => text,
        None => text,
    };

    let mut ranges = Vec::new();

    let token_text = if text.starts_with("#!") && !text.starts_with("#![") {
        let end = text.find('\n').map_or(text.len(), |offset| offset);

        ranges.push(0..end);

        format!(
            "{}{}",
            " ".repeat(text[..end].chars().count()),
            &text[end..]
        )
    } else {
        text.to_owned()
    };

    let tokens = token_text
        .parse::<TokenStream>()
        .map_err(|error| StyleError::Tokenization(error.to_string()))?;

    literals(tokens, &Source::new(text), text, &mut ranges)?;

    ranges.sort_by_key(|range| range.start);

    let text = strip_comments(text, &ranges)?;

    let file = syn::parse_file(&text)?;

    let mut attributes = Attributes::default();

    attributes.visit_file(&file);

    let source = Source::new(&text);

    let edits = attributes
        .spans
        .into_iter()
        .map(|span| {
            Ok(Edit {
                range: source.range(span)?,
                replacement: String::new(),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    apply(&text, edits)
}

#[derive(Default)]
struct Attributes {
    spans: Vec<Span>,
}

impl<'ast> Visit<'ast> for Attributes {
    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        if attribute.path().is_ident("doc") {
            self.spans.push(attribute.span());
        } else {
            visit::visit_attribute(self, attribute);
        }
    }
}

#[derive(Default)]
struct Boundaries {
    spans: Vec<(Span, Span)>,
    assignments: Vec<(Span, Span)>,
}

impl Boundaries {
    fn sequence<T: Spanned>(&mut self, items: &[T]) {
        self.spans.extend(
            items
                .windows(2)
                .map(|pair| (pair[0].span(), pair[1].span())),
        );
    }

    fn items(&mut self, items: &[Item]) {
        for pair in items.windows(2) {
            let grouped = matches!((&pair[0], &pair[1]), (Item::Use(_), Item::Use(_)))
                || matches!((&pair[0], &pair[1]), (Item::Mod(a), Item::Mod(b)) if a.content.is_none() && b.content.is_none());

            if !grouped {
                self.spans.push((pair[0].span(), pair[1].span()));
            }
        }
    }
}

impl<'ast> Visit<'ast> for Boundaries {
    fn visit_file(&mut self, file: &'ast syn::File) {
        self.items(&file.items);

        visit::visit_file(self, file);
    }

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if let Some((_, items)) = &item.content {
            self.items(items);
        }

        visit::visit_item_mod(self, item);
    }

    fn visit_block(&mut self, block: &'ast syn::Block) {
        for pair in block.stmts.windows(2) {
            let boundary = (pair[0].span(), pair[1].span());

            if assignment(&pair[0]) && assignment(&pair[1]) {
                self.assignments.push(boundary);
            } else {
                self.spans.push(boundary);
            }
        }

        visit::visit_block(self, block);
    }

    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        self.sequence(&item.items);

        visit::visit_item_impl(self, item);
    }

    fn visit_item_trait(&mut self, item: &'ast syn::ItemTrait) {
        self.sequence(&item.items);

        visit::visit_item_trait(self, item);
    }

    fn visit_item_foreign_mod(&mut self, item: &'ast syn::ItemForeignMod) {
        self.sequence(&item.items);

        visit::visit_item_foreign_mod(self, item);
    }
}

fn assignment(statement: &syn::Stmt) -> bool {
    fn expression(value: &syn::Expr) -> bool {
        match value {
            syn::Expr::Assign(_) => true,
            syn::Expr::Binary(binary) => matches!(
                binary.op,
                syn::BinOp::AddAssign(_)
                    | syn::BinOp::SubAssign(_)
                    | syn::BinOp::MulAssign(_)
                    | syn::BinOp::DivAssign(_)
                    | syn::BinOp::RemAssign(_)
                    | syn::BinOp::BitXorAssign(_)
                    | syn::BinOp::BitAndAssign(_)
                    | syn::BinOp::BitOrAssign(_)
                    | syn::BinOp::ShlAssign(_)
                    | syn::BinOp::ShrAssign(_)
            ),
            syn::Expr::Paren(parenthesized) => expression(&parenthesized.expr),
            syn::Expr::Group(group) => expression(&group.expr),
            _ => false,
        }
    }

    matches!(statement, syn::Stmt::Expr(value, _) if expression(value))
}

fn format(text: &str) -> Result<String> {
    let mut child = Command::new("rustfmt")
        .args([
            "--edition",
            "2024",
            "--emit",
            "stdout",
            "--config",
            "skip_children=true",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| StyleError::Io {
            operation: "Starting rustfmt",
            source,
        })?;

    let mut input = child.stdin.take().ok_or(StyleError::RustfmtInput)?;

    let output = std::thread::scope(|scope| {
        let writer = scope.spawn(move || input.write_all(text.as_bytes()));

        let output = child.wait_with_output().map_err(|source| StyleError::Io {
            operation: "Waiting for rustfmt",
            source,
        })?;

        writer
            .join()
            .map_err(|_| StyleError::RustfmtWriter)?
            .map_err(|source| StyleError::Io {
                operation: "Writing rustfmt input",
                source,
            })?;

        Ok::<_, StyleError>(output)
    })?;

    if !output.status.success() {
        return Err(StyleError::Rustfmt(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }

    String::from_utf8(output.stdout).map_err(StyleError::RustfmtOutput)
}

pub fn normalize(text: &str, crate_name: Option<&str>, preserve_comments: bool) -> Result<String> {
    let text = if preserve_comments {
        text.to_owned()
    } else {
        uncomment(text)?
    };

    let text = crate::crate_paths::replace(&text, crate_name)?;

    let text = format(&text)?;

    let file = syn::parse_file(&text)?;

    let mut boundaries = Boundaries::default();

    boundaries.visit_file(&file);

    let source = Source::new(&text);

    let mut edits = Vec::new();

    for (previous, next) in boundaries.spans {
        let range = source.range(previous)?.end..source.range(next)?.start;

        if let Some(edit) = spacing(&text, range, next.start().column)? {
            edits.push(edit);
        }
    }

    for (previous, next) in boundaries.assignments {
        let range = source.range(previous)?.end..source.range(next)?.start;

        if let Some(edit) = group_spacing(&text, range, next.start().column)? {
            edits.push(edit);
        }
    }

    apply(&text, edits)
}
