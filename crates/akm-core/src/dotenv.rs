use crate::error::{Error, Result};
use crate::model::is_valid_env_var;

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

/// Parse a dotenv document. Blank lines and `#` comments are ignored, `export`
/// prefixes are accepted, and a repeated name is an error rather than a silent
/// override.
pub fn parse(input: &str) -> Result<Vec<(String, String)>> {
    let mut out: Vec<(String, String)> = Vec::new();

    for (index, raw) in input.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let line = line.strip_prefix("export ").unwrap_or(line).trim_start();
        let (name, value) = line
            .split_once('=')
            .ok_or_else(|| Error::Invalid(format!("line {}: expected NAME=value", index + 1)))?;
        let name = name.trim();

        if !is_valid_env_var(name) {
            return Err(Error::Invalid(format!(
                "line {}: {name:?} is not a valid environment variable name",
                index + 1
            )));
        }

        if out.iter().any(|(existing, _)| existing == name) {
            return Err(Error::Invalid(format!(
                "line {}: {name} appears more than once",
                index + 1
            )));
        }

        out.push((name.to_string(), unquote(value.trim())?));
    }

    Ok(out)
}

fn unquote(raw: &str) -> Result<String> {
    if raw.len() >= 2 && raw.starts_with('\'') && raw.ends_with('\'') {
        return Ok(raw[1..raw.len() - 1].to_string());
    }

    if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
        let inner = &raw[1..raw.len() - 1];
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();

        while let Some(ch) = chars.next() {
            if ch != '\\' {
                out.push(ch);
                continue;
            }

            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        }

        return Ok(out);
    }

    Ok(raw.to_string())
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
