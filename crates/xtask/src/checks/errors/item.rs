// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::lex::Token;

#[derive(Default)]
pub(super) struct ItemFields {
    pub(super) sources: Vec<String>,
    pub(super) strings: Vec<String>,
}

pub(super) fn item_fields(tokens: &[Token], start: usize) -> ItemFields {
    let mut position = start;
    while let Some(token) = tokens.get(position) {
        if token.is_punct('#') {
            position = group_end(tokens, position + 1) + 1;
        } else if token.is_ident("pub") {
            position += 1;
            if tokens.get(position).is_some_and(|t| t.is_punct('(')) {
                position = group_end(tokens, position) + 1;
            }
        } else if token.is_ident("struct") {
            position += 1;
        } else {
            break;
        }
    }
    let named = tokens
        .get(position)
        .is_some_and(|t| t.ident().is_some() && !t.is_ident("enum"));
    if !named {
        return ItemFields::default();
    }
    position += 1;
    if tokens.get(position).is_some_and(|t| t.is_punct('<')) {
        position = angle_end(tokens, position) + 1;
    }
    match tokens.get(position) {
        Some(open) if open.is_punct('{') || open.is_punct('(') => {
            let close = group_end(tokens, position);
            fields(&tokens[position + 1..close], open.is_punct('('))
        }
        _ => ItemFields::default(),
    }
}

fn is_open(token: &Token) -> bool {
    token.is_punct('(') || token.is_punct('[') || token.is_punct('{')
}

fn is_close(token: &Token) -> bool {
    token.is_punct(')') || token.is_punct(']') || token.is_punct('}')
}

fn group_end(tokens: &[Token], open: usize) -> usize {
    let mut depth = 0usize;
    for (offset, token) in tokens[open..].iter().enumerate() {
        if is_open(token) {
            depth += 1;
        } else if is_close(token) {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return open + offset;
            }
        }
    }
    tokens.len().saturating_sub(1)
}

fn is_arrow(tokens: &[Token], index: usize) -> bool {
    tokens[index].is_punct('-') && tokens.get(index + 1).is_some_and(|t| t.is_punct('>'))
}

fn angle_end(tokens: &[Token], open: usize) -> usize {
    let mut depth = 0usize;
    let mut index = open;
    while index < tokens.len() {
        if is_arrow(tokens, index) {
            index += 1;
        } else if tokens[index].is_punct('<') {
            depth += 1;
        } else if tokens[index].is_punct('>') {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return index;
            }
        }
        index += 1;
    }
    tokens.len().saturating_sub(1)
}

fn split_fields(tokens: &[Token]) -> Vec<&[Token]> {
    let mut parts = Vec::new();
    let (mut start, mut depth, mut index) = (0usize, 0usize, 0usize);
    while index < tokens.len() {
        let token = &tokens[index];
        if is_arrow(tokens, index) {
            index += 1;
        } else if is_open(token) || token.is_punct('<') {
            depth += 1;
        } else if is_close(token) || token.is_punct('>') {
            depth = depth.saturating_sub(1);
        } else if token.is_punct(',') && depth == 0 {
            parts.push(&tokens[start..index]);
            start = index + 1;
        }
        index += 1;
    }
    parts.push(&tokens[start..]);
    parts.retain(|part| !part.is_empty());
    parts
}

fn fields(tokens: &[Token], tuple: bool) -> ItemFields {
    let mut found = ItemFields::default();
    for (index, field) in split_fields(tokens).into_iter().enumerate() {
        let mut position = 0;
        let mut attribute = false;
        while let Some(token) = field.get(position) {
            if token.is_punct('#') {
                let end = group_end(field, position + 1);
                attribute |= field[position + 1..=end]
                    .iter()
                    .any(|t| t.is_ident("source") || t.is_ident("from"));
                position = end + 1;
            } else if token.is_ident("pub") {
                position += 1;
                if field.get(position).is_some_and(|t| t.is_punct('(')) {
                    position = group_end(field, position) + 1;
                }
            } else {
                break;
            }
        }
        let named = !tuple
            && field.get(position).is_some_and(|t| t.ident().is_some())
            && field.get(position + 1).is_some_and(|t| t.is_punct(':'));
        let (name, kind) = match field.get(position).and_then(Token::ident) {
            Some(name) if named => (name.to_owned(), &field[position + 2..]),
            _ => (index.to_string(), &field[position..]),
        };
        if kind.len() == 1 && kind[0].is_ident("String") {
            found.strings.push(name.clone());
        }
        if attribute || name == "source" {
            found.sources.push(name);
        }
    }
    found
}
