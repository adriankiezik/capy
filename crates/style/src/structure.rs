use anyhow::{Context as _, Result};
use proc_macro2::{Delimiter, Span, TokenStream, TokenTree};
use std::path::Path;
use syn::{Item, Visibility, spanned::Spanned};

fn error_paths(tokens: TokenStream, spans: &mut Vec<Span>) {
    let tokens: Vec<_> = tokens.into_iter().collect();

    for (index, token) in tokens.iter().enumerate() {
        if let TokenTree::Group(group) = token {
            error_paths(group.stream(), spans);
        }

        if !matches!(token, TokenTree::Ident(name) if name == "thiserror")
            || !matches!(tokens.get(index + 1), Some(TokenTree::Punct(p)) if p.as_char() == ':')
            || !matches!(tokens.get(index + 2), Some(TokenTree::Punct(p)) if p.as_char() == ':')
        {
            continue;
        }

        let error = match tokens.get(index + 3) {
            Some(TokenTree::Ident(name)) => name == "Error",
            Some(TokenTree::Group(group)) if group.delimiter() == Delimiter::Brace => group
                .stream()
                .into_iter()
                .any(|token| matches!(token, TokenTree::Ident(name) if name == "Error")),
            _ => false,
        };

        if error {
            spans.push(token.span());
        }
    }
}

pub fn violations(text: &str, path: &Path) -> Result<Vec<(usize, &'static str)>> {
    let file = syn::parse_file(text).context("Parsing Rust structure")?;

    let name = path.file_name().and_then(|name| name.to_str());

    let mut violations = Vec::new();

    if name != Some("error.rs") {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);

        let text = if file.shebang.is_some() {
            text.find('\n').map_or("", |offset| &text[offset..])
        } else {
            text
        };

        let tokens = text
            .parse::<TokenStream>()
            .map_err(|error| anyhow::anyhow!("Rust tokenization failed: {error}"))?;

        let mut spans = Vec::new();

        error_paths(tokens, &mut spans);

        violations.extend(spans.into_iter().map(|span| {
            (
                span.start().line,
                "thiserror::Error is only allowed in error.rs",
            )
        }));
    }

    if matches!(name, Some("mod.rs" | "lib.rs")) {
        for item in &file.items {
            let allowed = match item {
                Item::Mod(module) => module.content.is_none(),
                Item::Use(import) => !matches!(import.vis, Visibility::Inherited),
                Item::ExternCrate(export) => !matches!(export.vis, Visibility::Inherited),
                _ => false,
            };

            if !allowed {
                violations.push((
                    item.span().start().line,
                    "mod.rs and lib.rs may only contain external module declarations and re-exports",
                ));
            }
        }
    }

    violations.sort_by_key(|(line, _)| *line);

    Ok(violations)
}
