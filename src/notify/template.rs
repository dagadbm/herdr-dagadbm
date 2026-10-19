//! Renders the notification templates with the same token syntax Herdr uses in
//! its own `config.toml`: `{token}` for a built-in value, `$name` for a custom
//! value the pane reported as metadata, and `{{` / `}}` for literal braces.
//!
//! A token without a value renders as an empty string, and nothing around it is
//! cleaned up: the configured string is exactly what is shown. An unknown
//! `{token}` is kept as written, so a typo stays visible in the notification.

use std::collections::BTreeMap;

/// The built-in tokens a template can use, as Herdr names them.
pub(crate) const TOKENS: &[&str] = &[
    "workspace",
    "tab",
    "pane",
    "agent",
    "terminal_title",
    "terminal_title_stripped",
    "state_icon",
    "state_text",
    "branch",
    "git_status",
    "hostname",
];

#[derive(Debug, Default)]
pub(crate) struct TokenValues {
    /// Values of the built-in tokens; a missing entry renders as empty.
    pub(crate) builtins: BTreeMap<&'static str, String>,
    /// Values of the `$name` tokens.
    pub(crate) metadata: BTreeMap<String, String>,
}

/// Renders the `{token}` at the start of `rest` and returns what follows it.
fn render_token<'a>(rest: &'a str, values: &TokenValues, output: &mut String) -> &'a str {
    match rest[1..].find('}') {
        Some(end) if TOKENS.contains(&&rest[1..=end]) => {
            if let Some(value) = values.builtins.get(&rest[1..=end]) {
                output.push_str(value);
            }
            &rest[end + 2..]
        }
        _ => {
            output.push('{');
            &rest[1..]
        }
    }
}

/// Renders the `$name` at the start of `rest` and returns what follows it.
fn render_metadata<'a>(rest: &'a str, values: &TokenValues, output: &mut String) -> &'a str {
    let name_len = rest[1..]
        .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .unwrap_or(rest.len() - 1);
    if name_len == 0 {
        output.push('$');
    } else if let Some(value) = values.metadata.get(&rest[1..=name_len]) {
        output.push_str(value);
    }
    &rest[1 + name_len..]
}

pub(crate) fn render(template: &str, values: &TokenValues) -> String {
    let mut output = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(ch) = rest.chars().next() {
        match ch {
            '{' if rest.starts_with("{{") => {
                output.push('{');
                rest = &rest[2..];
            }
            '}' if rest.starts_with("}}") => {
                output.push('}');
                rest = &rest[2..];
            }
            '{' => rest = render_token(rest, values, &mut output),
            '$' => rest = render_metadata(rest, values, &mut output),
            _ => {
                output.push(ch);
                rest = &rest[ch.len_utf8()..];
            }
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values() -> TokenValues {
        let mut values = TokenValues::default();
        values.builtins.insert("agent", "Codex".to_string());
        values.builtins.insert("workspace", "herdr".to_string());
        values
            .metadata
            .insert("jj_status".to_string(), "clean".to_string());
        values
    }

    #[test]
    fn replaces_builtin_tokens() {
        assert_eq!(
            render("{agent} in {workspace}", &values()),
            "Codex in herdr"
        );
    }

    #[test]
    fn missing_values_render_empty_without_cleanup() {
        assert_eq!(render("{agent} · {branch}", &values()), "Codex · ");
    }

    #[test]
    fn unknown_tokens_stay_as_written() {
        assert_eq!(render("{agnet} {agent}", &values()), "{agnet} Codex");
        assert_eq!(render("{unclosed", &values()), "{unclosed");
    }

    #[test]
    fn doubled_braces_are_literal() {
        assert_eq!(render("{{agent}} {agent}", &values()), "{agent} Codex");
    }

    #[test]
    fn dollar_names_render_pane_metadata() {
        assert_eq!(render("jj: $jj_status.", &values()), "jj: clean.");
        assert_eq!(render("$missing!", &values()), "!");
        assert_eq!(render("costs $ 5", &values()), "costs $ 5");
    }

    #[test]
    fn keeps_multibyte_text_intact() {
        assert_eq!(render("⚠ {agent} · ✓", &values()), "⚠ Codex · ✓");
    }
}
