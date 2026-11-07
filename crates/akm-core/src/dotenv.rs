use crate::error::{Error, Result};
use crate::model::is_valid_env_var;

/// The largest document `parse` reads. A larger one is refused before any line is read.
pub const MAX_BYTES: usize = 256 * 1024;
/// The longest physical line, without its line ending.
pub const MAX_LINE_BYTES: usize = 16 * 1024;
/// The most entries in one document.
pub const MAX_ENTRIES: usize = 500;

/// Render one `NAME=value` line, quoting only when the value needs it.
///
/// Single-quoted values are literal. Double-quoted values support the escapes
/// this module writes, so `parse` round-trips whatever `line` produces.
pub fn line(name: &str, value: &str) -> String {
    let safe = !value.is_empty()
        && value.chars().all(|ch| {
            ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | ':' | '@' | '+')
        });

    if safe {
        return format!("{name}={value}");
    }

    if value.contains('\'')
        || value.contains('"')
        || value.contains('\\')
        || value.contains('\n')
        || value.contains('\r')
    {
        let mut escaped = String::with_capacity(value.len() + 2);
        escaped.push('"');
        for ch in value.chars() {
            match ch {
                '\\' => escaped.push_str("\\\\"),
                '"' => escaped.push_str("\\\""),
                '\n' => escaped.push_str("\\n"),
                '\r' => escaped.push_str("\\r"),
                '\t' => escaped.push_str("\\t"),
                _ => escaped.push(ch),
            }
        }
        escaped.push('"');
        format!("{name}={escaped}")
    } else {
        format!("{name}='{value}'")
    }
}

/// Parse a dotenv document with the AKM profile (docs/design/unit3.md, section 2).
///
/// A leading byte order mark is dropped. A line ends at a line feed or a CR LF pair. Blank
/// lines and `#` comment lines are ignored, and so are `export ` prefixes. A value is
/// single-quoted (literal), double-quoted (escapes `\n \r \t \" \\`, may span lines) or
/// unquoted (a ` #` starts a comment). Nothing is executed or interpolated. A repeated name,
/// a malformed line or a document beyond the limits is an error, never a silent skip. An
/// error names the line and never repeats file content.
pub fn parse(input: &str) -> Result<Vec<(String, String)>> {
    if input.len() > MAX_BYTES {
        return Err(Error::Invalid(format!(
            "the file is larger than {} KiB ({} bytes)",
            MAX_BYTES / 1024,
            input.len()
        )));
    }
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    if let Some(at) = input.find('\0') {
        return Err(at_line(
            line_of(input, at),
            "contains a NUL character".into(),
        ));
    }

    let mut out: Vec<(String, String)> = Vec::new();
    let mut position = 0;

    while position < input.len() {
        let first = line_of(input, position);
        let (line_end, next) = match input[position..].find('\n') {
            Some(offset) => (position + offset, position + offset + 1),
            None => (input.len(), input.len()),
        };
        let line = without_cr(&input[position..line_end], line_end < input.len());
        check_length(line, first)?;
        let trimmed = line.trim_start_matches([' ', '\t']);
        if trimmed.is_empty() || trimmed.starts_with('#') {
            reject_lone_cr(line, first)?;
            position = next;
            continue;
        }

        let (head, after_equals) = trimmed
            .split_once('=')
            .ok_or_else(|| at_line(first, "expected NAME=value".into()))?;
        let name = strip_export(head).trim_end_matches([' ', '\t']);
        if !is_valid_env_var(name) {
            return Err(at_line(first, invalid_name(name)));
        }
        if out.iter().any(|(existing, _)| existing == name) {
            return Err(at_line(first, format!("{name} appears more than once")));
        }
        if out.len() == MAX_ENTRIES {
            return Err(Error::Invalid(format!(
                "line {first}: the file has more than {MAX_ENTRIES} entries"
            )));
        }

        let value_text = after_equals.trim_start_matches([' ', '\t']);
        let quote = value_text
            .chars()
            .next()
            .filter(|ch| matches!(ch, '"' | '\''));
        let (value, resume) = match quote {
            Some(quote) => {
                let value_start = offset_in(input, value_text) + 1;
                let (value, close) = quoted(input, value_start, quote, first)?;
                let tail_end = input[close..]
                    .find('\n')
                    .map_or(input.len(), |at| close + at);
                let tail = without_cr(&input[close..tail_end], tail_end < input.len());
                let trimmed_tail = tail.trim_start_matches([' ', '\t']);
                let close_line = line_of(input, close);
                if !(trimmed_tail.is_empty() || trimmed_tail.starts_with('#')) {
                    return Err(at_line(
                        close_line,
                        "unexpected text after the closing quote".into(),
                    ));
                }
                reject_lone_cr(tail, close_line)?;
                check_statement_lines(input, position, tail_end, first)?;
                (value, (tail_end + 1).min(input.len()))
            }
            None => {
                reject_lone_cr(line, first)?;
                let bytes = after_equals.as_bytes();
                let comment = after_equals
                    .char_indices()
                    .find(|&(index, ch)| {
                        ch == '#' && index > 0 && matches!(bytes[index - 1], b' ' | b'\t')
                    })
                    .map_or(after_equals.len(), |(index, _)| index);
                (
                    after_equals[..comment]
                        .trim_matches([' ', '\t'])
                        .to_string(),
                    next,
                )
            }
        };

        out.push((name.to_string(), value));
        position = resume;
    }

    Ok(out)
}

fn at_line(line: usize, message: String) -> Error {
    Error::Invalid(format!("line {line}: {message}"))
}

fn line_of(input: &str, at: usize) -> usize {
    input.as_bytes()[..at]
        .iter()
        .filter(|&&byte| byte == b'\n')
        .count()
        + 1
}

fn offset_in(input: &str, part: &str) -> usize {
    part.as_ptr() as usize - input.as_ptr() as usize
}

fn without_cr(line: &str, terminated: bool) -> &str {
    if terminated {
        line.strip_suffix('\r').unwrap_or(line)
    } else {
        line
    }
}

fn reject_lone_cr(line: &str, number: usize) -> Result<()> {
    if line.contains('\r') {
        return Err(at_line(
            number,
            "a carriage return without a line feed".into(),
        ));
    }
    Ok(())
}

fn check_length(line: &str, number: usize) -> Result<()> {
    if line.len() > MAX_LINE_BYTES {
        return Err(Error::Invalid(format!(
            "line {number} is longer than {} KiB",
            MAX_LINE_BYTES / 1024
        )));
    }
    Ok(())
}

fn check_statement_lines(input: &str, from: usize, to: usize, first: usize) -> Result<()> {
    for (index, line) in input[from..to].split('\n').enumerate() {
        check_length(line.strip_suffix('\r').unwrap_or(line), first + index)?;
    }
    Ok(())
}

fn strip_export(head: &str) -> &str {
    match head.strip_prefix("export") {
        Some(rest) if rest.starts_with([' ', '\t']) => rest.trim_start_matches([' ', '\t']),
        _ => head,
    }
}

/// A name is repeated in an error only when it looks like a name. Anything else could be
/// secret material.
fn invalid_name(name: &str) -> String {
    let plain = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
    if plain {
        format!("{name:?} is not a valid environment variable name (use A-Z, 0-9, _)")
    } else {
        "not a valid environment variable name (use A-Z, 0-9, _)".into()
    }
}

/// The text between the quotes that open just before `start`, and the index after the
/// closing quote.
fn quoted(input: &str, start: usize, quote: char, first: usize) -> Result<(String, usize)> {
    let unterminated = || at_line(first, "unterminated quote".into());
    if quote == '\'' {
        let end = input[start..].find('\'').ok_or_else(unterminated)?;
        return Ok((input[start..start + end].to_string(), start + end + 1));
    }

    let mut out = String::new();
    let mut chars = input[start..].char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        match ch {
            '"' => return Ok((out, start + index + 1)),
            '\\' => {
                let decoded = match chars.peek() {
                    Some((_, 'n')) => Some('\n'),
                    Some((_, 'r')) => Some('\r'),
                    Some((_, 't')) => Some('\t'),
                    Some((_, '"')) => Some('"'),
                    Some((_, '\\')) => Some('\\'),
                    _ => None,
                };
                match decoded {
                    Some(decoded) => {
                        out.push(decoded);
                        chars.next();
                    }
                    None => out.push('\\'),
                }
            }
            other => out.push(other),
        }
    }
    Err(unterminated())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_plain_values_without_quotes() {
        assert_eq!(line("A", "abc-123"), "A=abc-123");
        assert_eq!(line("A", ""), "A=''");
    }

    #[test]
    fn quotes_and_round_trips_awkward_values() {
        for value in ["a b", "it's", "say \"hi\"", "back\\slash", "line\nbreak"] {
            let rendered = line("A", value);
            let parsed = parse(&rendered).unwrap();
            assert_eq!(parsed, vec![("A".to_string(), value.to_string())]);
        }
    }

    #[test]
    fn parses_comments_export_and_double_quotes() {
        let parsed =
            parse("# comment\n\nexport OPENAI=sk-123\nGITHUB='ghp 456'\nMULTI=\"a\\nb\"\n")
                .unwrap();

        assert_eq!(
            parsed,
            vec![
                ("OPENAI".to_string(), "sk-123".to_string()),
                ("GITHUB".to_string(), "ghp 456".to_string()),
                ("MULTI".to_string(), "a\nb".to_string()),
            ]
        );
    }

    #[test]
    fn rejects_bad_names_duplicates_and_missing_equals() {
        assert!(parse("lower=1").is_err());
        assert!(parse("A=1\nA=2").is_err());
        assert!(parse("JUSTAWORD").is_err());
    }
}
