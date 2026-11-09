//! Decode a Bash AST ANSI-C quoted word without evaluating shell code.
use crate::Result;
use std::{iter::Peekable, str::Chars};

pub(super) fn decode(source: &str) -> Result<String> {
    let source = source
        .strip_prefix("$'")
        .and_then(|s| s.strip_suffix('\''))
        .ok_or("invalid ANSI-C quoted word")?;
    let mut chars = source.chars().peekable();
    let mut value = String::new();
    while let Some(character) = chars.next() {
        let character = if character == '\\' {
            escape(&mut chars)?
        } else {
            character
        };
        if character == '\0' {
            return Err("ANSI-C NUL truncation is unsupported".into());
        }
        value.push(character);
    }
    Ok(value)
}
fn escape(chars: &mut Peekable<Chars<'_>>) -> Result<char> {
    let escape = chars.next().ok_or("incomplete ANSI-C escape")?;
    const SIMPLE: [(char, char); 13] = [
        ('a', '\u{7}'),
        ('b', '\u{8}'),
        ('e', '\u{1b}'),
        ('E', '\u{1b}'),
        ('f', '\u{c}'),
        ('n', '\n'),
        ('r', '\r'),
        ('t', '\t'),
        ('v', '\u{b}'),
        ('\\', '\\'),
        ('\'', '\''),
        ('"', '"'),
        ('?', '?'),
    ];
    if let Some((_, value)) = SIMPLE.iter().find(|(key, _)| *key == escape) {
        return Ok(*value);
    }
    if escape == 'c' {
        return control(chars);
    }
    numeric(escape, chars)
}
fn control(chars: &mut Peekable<Chars<'_>>) -> Result<char> {
    let character = chars.next().ok_or("ANSI-C control escape is incomplete")?;
    if !character.is_ascii() {
        return Err("non-ASCII ANSI-C control escape is unsupported".into());
    }
    let byte = if character == '?' {
        127
    } else {
        (character as u8) & 31
    };
    Ok(char::from(byte))
}
fn numeric(escape: char, chars: &mut Peekable<Chars<'_>>) -> Result<char> {
    let (radix, width, initial, unicode) = match escape {
        '0'..='7' => (8, 3, escape.to_digit(8), false),
        'x' => (16, 2, None, false),
        'u' => (16, 4, None, true),
        'U' => (16, 8, None, true),
        _ => return Err("unsupported ANSI-C escape; no literal argv was inferred".into()),
    };
    let value =
        digits(chars, radix, width, initial).ok_or("ANSI-C numeric escape has no digits")?;
    if unicode {
        return char::from_u32(value).ok_or_else(|| "invalid ANSI-C Unicode escape".into());
    }
    let byte = (value & 255) as u8;
    if !byte.is_ascii() {
        return Err("non-UTF8 ANSI-C byte escape is unsupported".into());
    }
    Ok(char::from(byte))
}
fn digits(
    chars: &mut Peekable<Chars<'_>>,
    radix: u32,
    width: usize,
    initial: Option<u32>,
) -> Option<u32> {
    let mut value = initial.unwrap_or(0);
    let mut count = usize::from(initial.is_some());
    let remaining = width - count;
    for _ in 0..remaining {
        let Some(digit) = chars.peek().and_then(|character| character.to_digit(radix)) else {
            break;
        };
        chars.next();
        value = value * radix + digit;
        count += 1;
    }
    (count > 0).then_some(value)
}
