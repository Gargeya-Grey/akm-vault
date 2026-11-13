use std::collections::HashSet;

use zeroize::Zeroizing;

use crate::error::{Error, Result};
use crate::model::is_valid_env_var;

/// The largest document `parse` reads. A larger one is refused before any line is read.
pub const MAX_BYTES: usize = 256 * 1024;
/// The longest physical line, without its line ending.
pub const MAX_LINE_BYTES: usize = 16 * 1024;
/// The most entries in one document.
pub const MAX_ENTRIES: usize = 500;

#[cfg(test)]
thread_local! {
    static WORK: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Counts the bytes that `parse` scans to number its lines and the name comparisons that it
/// makes, so that a test can prove a bound on its work by a count and not by a clock.
fn work(steps: usize) {
    #[cfg(test)]
    WORK.with(|work| work.set(work.get().saturating_add(steps)));
    let _ = steps;
}

/// Render one `NAME=value` line, quoting only when the value needs it.
///
/// Single-quoted values are literal. Double-quoted values support the escapes
/// this module writes, so `parse` round-trips whatever `line` produces.
///
/// The line is written into one buffer of its exact length, so the value is never copied
/// into a buffer that is freed without clearing: no intermediate string, and no old buffer of
/// a string that grew (FV-10). The caller clears the line that it gets.
pub fn line(name: &str, value: &str) -> String {
    let safe = !value.is_empty()
        && value.chars().all(|ch| {
            ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | ':' | '@' | '+')
        });
    let double = !safe && value.contains(['\'', '"', '\\', '\n', '\r']);
    let escape = |ch: char| match ch {
        '\\' => Some("\\\\"),
        '"' => Some("\\\""),
        '\n' => Some("\\n"),
        '\r' => Some("\\r"),
        '\t' => Some("\\t"),
        _ => None,
    };
    let value_length: usize = if double {
        value
            .chars()
            .map(|ch| escape(ch).map_or(ch.len_utf8(), str::len))
            .sum()
    } else {
        value.len()
    };
    let quotes = if safe { 0 } else { 2 };
    let mut out = String::with_capacity(name.len() + 1 + quotes + value_length);
    out.push_str(name);
    out.push('=');
    if safe {
        out.push_str(value);
    } else if double {
        out.push('"');
        for ch in value.chars() {
            match escape(ch) {
                Some(text) => out.push_str(text),
                None => out.push(ch),
            }
        }
        out.push('"');
    } else {
        out.push('\'');
        out.push_str(value);
        out.push('\'');
    }
    out
}

/// Parse a dotenv document with the AKM profile (docs/design/unit3.md, section 2).
///
/// A leading byte order mark is dropped. A line ends at a line feed or a CR LF pair. Blank
/// lines and `#` comment lines are ignored, and so are `export ` prefixes. A value is
/// single-quoted (literal), double-quoted (escapes `\n \r \t \" \\`, may span lines) or
/// unquoted (a ` #` starts a comment). Nothing is executed or interpolated. A repeated name,
/// a malformed line or a document beyond the limits is an error, never a silent skip. An
/// error names the line and never repeats file content.
///
/// Each value is read into a buffer of its exact length, which never grows, and stays in a
/// buffer that is cleared when it is dropped until `parse` returns it: the entries read so
/// far, and the value being read, are cleared on every way out, also an error (FW-09). The
/// caller clears the text that it passes and the entries that it gets.
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

    let mut out: Zeroizing<Vec<(String, String)>> = Zeroizing::new(Vec::new());
    let mut names: HashSet<&str> = HashSet::new();
    let mut position = 0;
    // The number of the line that starts at `position`. It is counted as the parse goes, so
    // the work stays linear in the size of the document (CORE-R2-06).
    let mut line_number = 1;

    while position < input.len() {
        let first = line_number;
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
            line_number += 1;
            continue;
        }

        let (head, after_equals) = trimmed
            .split_once('=')
            .ok_or_else(|| at_line(first, "expected NAME=value".into()))?;
        let name = strip_export(head).trim_end_matches([' ', '\t']);
        if !is_valid_env_var(name) {
            return Err(at_line(first, invalid_name(name)));
        }
        work(1);
        if !names.insert(name) {
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
        let (mut value, resume, next_line) = match quote {
            Some(quote) => {
                let value_start = offset_in(input, value_text) + 1;
                let (value, close) = quoted(input, value_start, quote, first)?;
                let tail_end = input[close..]
                    .find('\n')
                    .map_or(input.len(), |at| close + at);
                let tail = without_cr(&input[close..tail_end], tail_end < input.len());
                let trimmed_tail = tail.trim_start_matches([' ', '\t']);
                let close_line = first + newlines(&input[position..close]);
                if !(trimmed_tail.is_empty() || trimmed_tail.starts_with('#')) {
                    return Err(at_line(
                        close_line,
                        "unexpected text after the closing quote".into(),
                    ));
                }
                reject_lone_cr(tail, close_line)?;
                check_statement_lines(input, position, tail_end, first)?;
                (value, (tail_end + 1).min(input.len()), close_line + 1)
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
                    Zeroizing::new(
                        after_equals[..comment]
                            .trim_matches([' ', '\t'])
                            .to_string(),
                    ),
                    next,
                    first + 1,
                )
            }
        };

        out.push((name.to_string(), std::mem::take(&mut *value)));
        position = resume;
        line_number = next_line;
    }

    Ok(std::mem::take(&mut *out))
}

fn at_line(line: usize, message: String) -> Error {
    Error::Invalid(format!("line {line}: {message}"))
}

/// The number of the line that holds byte `at`. For one error only: the parse counts its
/// lines as it goes.
fn line_of(input: &str, at: usize) -> usize {
    newlines(&input[..at]) + 1
}

fn newlines(text: &str) -> usize {
    work(text.len());
    text.bytes().filter(|&byte| byte == b'\n').count()
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
/// closing quote. A double-quoted value is measured first, then decoded into one buffer of
/// that length.
fn quoted(
    input: &str,
    start: usize,
    quote: char,
    first: usize,
) -> Result<(Zeroizing<String>, usize)> {
    let unterminated = || at_line(first, "unterminated quote".into());
    if quote == '\'' {
        let end = input[start..].find('\'').ok_or_else(unterminated)?;
        let value = Zeroizing::new(input[start..start + end].to_string());
        return Ok((value, start + end + 1));
    }

    let mut length = 0;
    let close = decode(&input[start..], |ch| length += ch.len_utf8()).ok_or_else(unterminated)?;
    let mut value = Zeroizing::new(String::with_capacity(length));
    decode(&input[start..], |ch| value.push(ch));
    Ok((value, start + close + 1))
}

/// Decodes a double-quoted value from the start of `text`, one character at a time into
/// `push`. The index of the closing quote, or `None` when there is none.
fn decode(text: &str, mut push: impl FnMut(char)) -> Option<usize> {
    let mut chars = text.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        match ch {
            '"' => return Some(index),
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
                        push(decoded);
                        chars.next();
                    }
                    None => push('\\'),
                }
            }
            other => push(other),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counted<T>(run: impl FnOnce() -> T) -> (T, usize) {
        WORK.with(|work| work.set(0));
        let result = run();
        (result, WORK.with(std::cell::Cell::get))
    }

    /// CORE-R2-06 sweep: the dotenv text comes from a file that AKM did not write. Within the
    /// size and entry limits, `parse` does work linear in the size of the document: it
    /// numbers its lines as it goes, and finds a repeated name without a scan of the names
    /// before it. Proved by a count of the steps, not by a clock.
    #[test]
    fn parse_does_linear_work_on_a_document_within_the_limits() {
        let mut blank = "\n".repeat(MAX_BYTES - 64);
        blank.push_str("LAST=1\n");
        let (parsed, steps) = counted(|| parse(&blank));
        assert_eq!(parsed.unwrap().len(), 1);
        assert!(
            steps <= 2 * blank.len(),
            "{steps} steps for {} bytes",
            blank.len()
        );

        let names: String = (0..MAX_ENTRIES)
            .map(|index| format!("NAME_{index}=value\n"))
            .collect();
        let (parsed, steps) = counted(|| parse(&names));
        assert_eq!(parsed.unwrap().len(), MAX_ENTRIES);
        assert!(
            steps <= 2 * names.len(),
            "{steps} steps for {} bytes",
            names.len()
        );

        let quoted: String = (0..MAX_ENTRIES)
            .map(|index| format!("Q_{index}=\"a\nb\"\n"))
            .collect();
        let (parsed, steps) = counted(|| parse(&quoted));
        assert_eq!(parsed.unwrap().len(), MAX_ENTRIES);
        assert!(
            steps <= 2 * quoted.len(),
            "{steps} steps for {} bytes",
            quoted.len()
        );
    }

    /// The line numbers in errors stay right after the change to counting as it goes.
    #[test]
    fn errors_name_the_right_line_after_blank_lines_comments_and_multiline_values() {
        for (text, line) in [
            (
                "
# c
A=1
B=\"x
y\"

lower=2
",
                "line 7:",
            ),
            (
                "A=1
B=\"x
y\" junk
",
                "line 3:",
            ),
            (
                "A=1

A=2
",
                "line 3:",
            ),
            (
                "A=1
B=\"open

",
                "line 2:",
            ),
        ] {
            let message = parse(text).unwrap_err().to_string();
            assert!(message.contains(line), "{text:?}: {message}");
        }
    }
}
