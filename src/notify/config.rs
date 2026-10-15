//! The `[notify]` section of the plugin's `config.toml`, in
//! `$HERDR_PLUGIN_CONFIG_DIR`. Every key is optional and falls back to the
//! defaults below, so a missing file, a missing section, or a file that does
//! not parse all behave like the defaults. The file is read on every
//! invocation, so edits apply to the next event without a reload.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

use serde::Deserialize;

pub(crate) const DEFAULT_STATUSES: &[&str] = &["blocked", "done"];
pub(crate) const DEFAULT_TITLE: &str = "{state_icon} 📂 {workspace} · 📑 {tab}";
pub(crate) const DEFAULT_SUBTITLE: &str = "";
pub(crate) const DEFAULT_MESSAGE: &str = "{terminal_title_stripped}";
pub(crate) const DEFAULT_ACTION_LABEL: &str = "Focus";
pub(crate) const DEFAULT_CLOSE_LABEL: &str = "Dismiss";

/// The commented config `notify-init-config` writes. It spells out every
/// default, so the file documents itself.
pub(crate) const EXAMPLE_CONFIG: &str = include_str!("../../config.example.toml");

#[derive(Debug, Default, Deserialize)]
struct ConfigFile {
    notify: Option<NotifySection>,
}

#[derive(Debug, Default, Deserialize)]
struct NotifySection {
    notify_on: Option<Vec<String>>,
    #[serde(flatten)]
    templates: TemplateOverrides,
    sound: Option<String>,
    timeout: Option<u64>,
    #[serde(default)]
    status: BTreeMap<String, TemplateOverrides>,
    #[serde(default)]
    state_icons: BTreeMap<String, String>,
}

#[derive(Debug, Default, Clone, Deserialize)]
struct TemplateOverrides {
    title: Option<String>,
    subtitle: Option<String>,
    message: Option<String>,
    action_label: Option<String>,
    close_label: Option<String>,
}

/// The templates one notification is rendered from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Templates {
    pub(crate) title: String,
    pub(crate) subtitle: String,
    pub(crate) message: String,
    pub(crate) action_label: String,
    pub(crate) close_label: String,
}

#[derive(Debug)]
pub(crate) struct NotifyConfig {
    statuses: Vec<String>,
    templates: TemplateOverrides,
    per_status: BTreeMap<String, TemplateOverrides>,
    /// `{state_icon}` replacements per status; unlisted statuses keep the
    /// built-in glyphs.
    state_icons: BTreeMap<String, String>,
    /// An alerter sound name; None plays no sound.
    pub(crate) sound: Option<String>,
    /// Seconds before an unclicked notification is dismissed; 0 keeps it.
    pub(crate) timeout_secs: u64,
}

impl Default for NotifyConfig {
    fn default() -> Self {
        Self::from_section(NotifySection::default())
    }
}

impl NotifyConfig {
    fn from_section(section: NotifySection) -> Self {
        Self {
            statuses: section
                .notify_on
                .unwrap_or_else(|| DEFAULT_STATUSES.iter().map(|s| s.to_string()).collect())
                .into_iter()
                .map(|status| status.trim().to_ascii_lowercase())
                .collect(),
            templates: section.templates,
            per_status: section
                .status
                .into_iter()
                .map(|(status, templates)| (status.trim().to_ascii_lowercase(), templates))
                .collect(),
            sound: section.sound.filter(|sound| !sound.trim().is_empty()),
            timeout_secs: section.timeout.unwrap_or(0),
            state_icons: section
                .state_icons
                .into_iter()
                .map(|(status, icon)| (status.trim().to_ascii_lowercase(), icon))
                .collect(),
        }
    }

    /// The configured `{state_icon}` for `status`, if any.
    pub(crate) fn state_icon(&self, status: &str) -> Option<&str> {
        self.state_icons.get(status).map(String::as_str)
    }

    pub(crate) fn status_is_enabled(&self, status: &str) -> bool {
        self.statuses.iter().any(|enabled| enabled == status)
    }

    /// The templates for `status`: its own `[notify.status.<status>]` keys,
    /// then the `[notify]` keys, then the built-in defaults.
    pub(crate) fn templates_for(&self, status: &str) -> Templates {
        let own = self.per_status.get(status);
        let pick = |field: fn(&TemplateOverrides) -> &Option<String>, default: &str| {
            own.and_then(|own| field(own).clone())
                .or_else(|| field(&self.templates).clone())
                .unwrap_or_else(|| default.to_string())
        };

        Templates {
            title: pick(|t| &t.title, DEFAULT_TITLE),
            subtitle: pick(|t| &t.subtitle, DEFAULT_SUBTITLE),
            message: pick(|t| &t.message, DEFAULT_MESSAGE),
            action_label: pick(|t| &t.action_label, DEFAULT_ACTION_LABEL),
            close_label: pick(|t| &t.close_label, DEFAULT_CLOSE_LABEL),
        }
    }
}

pub(crate) fn config_path() -> Option<PathBuf> {
    env::var_os("HERDR_PLUGIN_CONFIG_DIR").map(|dir| PathBuf::from(dir).join("config.toml"))
}

/// Loads the config, falling back to the defaults. A file that cannot be read
/// or parsed is reported on stderr, which Herdr keeps in the plugin log, and
/// never stops a notification.
pub(crate) fn load() -> NotifyConfig {
    let Some(path) = config_path() else {
        return NotifyConfig::default();
    };
    match fs::read_to_string(&path) {
        Ok(content) => parse(&content).unwrap_or_else(|err| {
            eprintln!("dagadbm: ignoring invalid config {}: {err}", path.display());
            NotifyConfig::default()
        }),
        Err(err) if err.kind() == io::ErrorKind::NotFound => NotifyConfig::default(),
        Err(err) => {
            eprintln!("dagadbm: cannot read config {}: {err}", path.display());
            NotifyConfig::default()
        }
    }
}

fn parse(content: &str) -> Result<NotifyConfig, String> {
    let file: ConfigFile = toml::from_str(content).map_err(|err| err.to_string())?;
    Ok(NotifyConfig::from_section(file.notify.unwrap_or_default()))
}

/// Writes the commented example config unless a config file already exists.
/// Returns the path and whether it was written.
pub(crate) fn init_config() -> Result<(PathBuf, bool), String> {
    let path = config_path().ok_or("HERDR_PLUGIN_CONFIG_DIR is not set")?;
    if path.exists() {
        return Ok((path, false));
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)
            .map_err(|err| format!("failed to create {}: {err}", dir.display()))?;
    }
    fs::write(&path, EXAMPLE_CONFIG)
        .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    Ok((path, true))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_apply_without_a_notify_section() {
        let config = parse("").unwrap();

        assert!(config.status_is_enabled("blocked"));
        assert!(config.status_is_enabled("done"));
        assert!(!config.status_is_enabled("working"));
        assert_eq!(config.sound, None);
        assert_eq!(config.timeout_secs, 0);
        assert_eq!(
            config.templates_for("done"),
            Templates {
                title: DEFAULT_TITLE.to_string(),
                subtitle: DEFAULT_SUBTITLE.to_string(),
                message: DEFAULT_MESSAGE.to_string(),
                action_label: DEFAULT_ACTION_LABEL.to_string(),
                close_label: DEFAULT_CLOSE_LABEL.to_string(),
            }
        );
    }

    #[test]
    fn status_overrides_win_over_section_keys_and_defaults() {
        let config = parse(
            r#"
            [notify]
            notify_on = ["Blocked", "working"]
            title = "{agent}"
            sound = "Ping"
            timeout = 60

            [notify.status.blocked]
            title = "needs you"
            message = "{tab}"
            "#,
        )
        .unwrap();

        assert!(config.status_is_enabled("blocked"));
        assert!(config.status_is_enabled("working"));
        assert!(!config.status_is_enabled("done"));
        assert_eq!(config.sound.as_deref(), Some("Ping"));
        assert_eq!(config.timeout_secs, 60);

        let blocked = config.templates_for("blocked");
        assert_eq!(blocked.title, "needs you");
        assert_eq!(blocked.message, "{tab}");
        assert_eq!(blocked.subtitle, DEFAULT_SUBTITLE);

        assert_eq!(config.templates_for("working").title, "{agent}");
    }

    #[test]
    fn state_icons_replace_only_the_listed_statuses() {
        let config = parse("[notify.state_icons]\nDone = \"✅\"\nblocked = \"🚨\"").unwrap();

        assert_eq!(config.state_icon("done"), Some("✅"));
        assert_eq!(config.state_icon("blocked"), Some("🚨"));
        assert_eq!(config.state_icon("working"), None);
        assert_eq!(NotifyConfig::default().state_icon("done"), None);
    }

    #[test]
    fn an_empty_sound_plays_nothing() {
        assert_eq!(parse("[notify]\nsound = \"\"").unwrap().sound, None);
    }

    #[test]
    fn invalid_config_is_an_error() {
        assert!(parse("[notify]\ntimeout = \"soon\"").is_err());
    }

    #[test]
    fn the_example_config_parses_to_the_defaults() {
        let config = parse(EXAMPLE_CONFIG).unwrap();
        let defaults = NotifyConfig::default();

        for status in ["blocked", "done", "working"] {
            assert_eq!(
                config.status_is_enabled(status),
                defaults.status_is_enabled(status)
            );
            assert_eq!(config.templates_for(status), defaults.templates_for(status));
        }
        assert_eq!(config.sound, defaults.sound);
        assert_eq!(config.timeout_secs, defaults.timeout_secs);
    }
}
