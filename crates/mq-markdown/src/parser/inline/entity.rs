//! Character references (`&amp;`, `&#35;`, `&#x23;`) and backslash escapes.

use super::entities::ENTITIES;

/// Decodes the character reference that starts at `pos`, returning the offset after it and its value.
pub(super) fn decode(src: &str, pos: usize) -> Option<(usize, String)> {
    let bytes = src.as_bytes();
    debug_assert_eq!(bytes[pos], b'&');
    let rest = &bytes[pos + 1..];

    if rest.first() == Some(&b'#') {
        let (radix, digits_from, max) = match rest.get(1) {
            Some(b'x' | b'X') => (16, 2, 6),
            _ => (10, 1, 7),
        };
        let digits = rest[digits_from..]
            .iter()
            .take_while(|b| (radix == 16 && b.is_ascii_hexdigit()) || b.is_ascii_digit())
            .count();
        if digits == 0 || digits > max || rest.get(digits_from + digits) != Some(&b';') {
            return None;
        }
        let number = u32::from_str_radix(&src[pos + 1 + digits_from..pos + 1 + digits_from + digits], radix).ok()?;
        let char = match char::from_u32(number) {
            Some(char) if number != 0 => char,
            _ => char::REPLACEMENT_CHARACTER,
        };
        return Some((pos + 1 + digits_from + digits + 1, char.to_string()));
    }

    let name = rest.iter().take_while(|b| b.is_ascii_alphanumeric()).count();
    if name == 0 || name > 31 || rest.get(name) != Some(&b';') {
        return None;
    }
    let name = &src[pos + 1..pos + 1 + name];
    let index = ENTITIES
        .binary_search_by(|(key, _)| key.as_bytes().cmp(name.as_bytes()))
        .ok()?;
    Some((pos + 1 + name.len() + 1, ENTITIES[index].1.to_string()))
}

/// Decodes backslash escapes and character references, as in link destinations and titles.
pub(in crate::parser) fn unescape(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut result = String::with_capacity(value.len());
    let mut from = 0;
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'\\' if bytes.get(index + 1).is_some_and(u8::is_ascii_punctuation) => {
                result.push_str(&value[from..index]);
                result.push(bytes[index + 1] as char);
                index += 2;
                from = index;
            }
            b'&' => match decode(value, index) {
                Some((end, decoded)) => {
                    result.push_str(&value[from..index]);
                    result.push_str(&decoded);
                    index = end;
                    from = index;
                }
                None => index += 1,
            },
            _ => index += 1,
        }
    }
    result.push_str(&value[from..]);

    result
}

/// Decodes character references only, as in the string values of JSX attributes.
pub(in crate::parser) fn decode_references(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut result = String::with_capacity(value.len());
    let mut from = 0;
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'&'
            && let Some((end, decoded)) = decode(value, index)
        {
            result.push_str(&value[from..index]);
            result.push_str(&decoded);
            index = end;
            from = index;
        } else {
            index += 1;
        }
    }
    result.push_str(&value[from..]);

    result
}

/// Removes the indentation that follows line endings, as labels of definitions and references show it.
pub(in crate::parser) fn remove_line_indent(value: &str) -> std::borrow::Cow<'_, str> {
    if !value.contains(['\n', '\r']) {
        return std::borrow::Cow::Borrowed(value);
    }
    let mut result = String::with_capacity(value.len());
    let mut after_line_ending = false;
    for char in value.chars() {
        match char {
            ' ' | '\t' if after_line_ending => {}
            _ => {
                after_line_ending = matches!(char, '\n' | '\r');
                result.push(char);
            }
        }
    }
    std::borrow::Cow::Owned(result)
}
