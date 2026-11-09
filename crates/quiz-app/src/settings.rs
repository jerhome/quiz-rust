// Settings are stored in ~/.config/quiz-rust/config.toml. The API key is written only
// when the user ticked "remember"; neither the key nor the file contents are ever logged.

use std::fs::{self, DirBuilder, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use quiz_core::{Difficulty, Lang};
use quiz_llm::{validate_model_id, DEFAULT_MODEL};
use serde::{Deserialize, Serialize};

use crate::i18n::lang_from_locale;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub ui_lang: Lang,
    pub quiz_lang: Lang,
    pub model: String,
    pub difficulty: Difficulty,
    pub count: u8,
    // 0 = no time limit.
    pub time_limit_secs: u32,
    pub name: String,
    pub remember_key: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

// Manual Debug so the API key can never leak through `{:?}`.
impl std::fmt::Debug for Settings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Settings")
            .field("ui_lang", &self.ui_lang)
            .field("quiz_lang", &self.quiz_lang)
            .field("model", &self.model)
            .field("difficulty", &self.difficulty)
            .field("count", &self.count)
            .field("time_limit_secs", &self.time_limit_secs)
            .field("name", &self.name)
            .field("remember_key", &self.remember_key)
            .field("api_key", &self.api_key.as_ref().map(|_| "***"))
            .finish()
    }
}

impl Default for Settings {
    fn default() -> Self {
        let lang = std::env::var("LANG")
            .map(|l| lang_from_locale(&l))
            .unwrap_or_default();
        Self {
            ui_lang: lang,
            quiz_lang: lang,
            model: DEFAULT_MODEL.to_string(),
            difficulty: Difficulty::Moyen,
            count: 5,
            time_limit_secs: 20,
            name: String::new(),
            remember_key: false,
            api_key: None,
        }
    }
}

impl Settings {
    fn sanitized(mut self) -> Self {
        if validate_model_id(&self.model).is_err() {
            self.model = DEFAULT_MODEL.to_string();
        }
        self.count = self.count.clamp(
            *quiz_core::COUNT_RANGE.start(),
            *quiz_core::COUNT_RANGE.end(),
        );
        if !self.remember_key {
            self.api_key = None;
        }
        self
    }

    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        let mut s = self.clone();
        if !s.remember_key {
            s.api_key = None;
        }
        toml::to_string_pretty(&s)
    }
}

pub fn config_path_from(xdg: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    let base = match xdg.filter(|x| x.starts_with('/')) {
        Some(x) => PathBuf::from(x),
        None => PathBuf::from(home.filter(|h| !h.is_empty())?).join(".config"),
    };
    Some(base.join("quiz-rust").join("config.toml"))
}

pub fn config_path() -> Option<PathBuf> {
    config_path_from(
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
    )
}

pub fn load_from(path: &Path) -> Settings {
    match fs::read_to_string(path) {
        Ok(text) => match toml::from_str::<Settings>(&text) {
            Ok(s) => s.sanitized(),
            Err(_) => {
                tracing::warn!("fichier de réglages invalide, valeurs par défaut utilisées");
                Settings::default()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
        Err(_) => {
            tracing::warn!("lecture des réglages impossible, valeurs par défaut utilisées");
            Settings::default()
        }
    }
}

// Atomic write: a temp file created with mode 0600 (never readable by others, even briefly)
// in a 0700 directory, then renamed over the target.
pub fn save_to(path: &Path, settings: &Settings) -> std::io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::other("chemin sans dossier"))?;
    DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    let text = settings
        .to_toml()
        .map_err(|_| std::io::Error::other("sérialisation impossible"))?;
    let tmp = dir.join(format!(".config.toml.{}.tmp", std::process::id()));
    let _ = fs::remove_file(&tmp);
    let result = (|| {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

// Initial value of the key field: ANTHROPIC_API_KEY > saved key > empty.
// Whatever the user types afterwards overrides it.
pub fn initial_key(env: Option<&str>, saved: Option<&str>) -> String {
    [env, saved]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|k| !k.is_empty())
        .unwrap_or_default()
        .to_string()
}

// Same value as ANTHROPIC_API_KEY: nothing to remember.
pub fn key_is_from_env(key: &str, env: Option<&str>) -> bool {
    let key = key.trim();
    !key.is_empty() && env.is_some_and(|e| e.trim() == key)
}

// The environment key is never written to the file; an already saved key is kept as is.
// A different key typed by hand is remembered normally.
pub fn key_to_persist(
    remember: bool,
    key: &str,
    env: Option<&str>,
    already_saved: Option<&str>,
) -> Option<String> {
    if !remember {
        return None;
    }
    if key_is_from_env(key, env) {
        return already_saved.map(str::to_string);
    }
    let key = key.trim();
    (!key.is_empty()).then(|| key.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("quiz-rust-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn sample() -> Settings {
        Settings {
            ui_lang: Lang::En,
            quiz_lang: Lang::Fr,
            model: "claude-sonnet-5-5".into(),
            difficulty: Difficulty::Difficile,
            count: 12,
            time_limit_secs: 0,
            name: "Zoé".into(),
            remember_key: true,
            api_key: Some("sk-test".into()),
        }
    }

    #[test]
    fn aller_retour() {
        let dir = temp_dir("rt");
        let path = dir.join("quiz-rust").join("config.toml");
        save_to(&path, &sample()).unwrap();
        assert_eq!(load_from(&path), sample());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn fichier_invalide_ou_absent() {
        let dir = temp_dir("bad");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        assert_eq!(load_from(&path), Settings::default());
        fs::write(&path, "ceci n'est pas = du toml {{{").unwrap();
        assert_eq!(load_from(&path), Settings::default());
        fs::write(&path, "model = \"a b\"\ncount = 99\n").unwrap();
        let s = load_from(&path);
        assert_eq!(s.model, DEFAULT_MODEL);
        assert_eq!(s.count, 20);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn permissions_600_et_700() {
        let dir = temp_dir("perm");
        let path = dir.join("quiz-rust").join("config.toml");
        save_to(&path, &sample()).unwrap();
        let file = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        let folder = fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(file, 0o600);
        assert_eq!(folder, 0o700);
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cle_ecrite_seulement_si_memorisee() {
        let mut s = sample();
        assert!(s.to_toml().unwrap().contains("sk-test"));
        s.remember_key = false;
        let text = s.to_toml().unwrap();
        assert!(!text.contains("sk-test") && !text.contains("api_key"));
        let dir = temp_dir("forget");
        let path = dir.join("c").join("config.toml");
        save_to(&path, &sample()).unwrap();
        save_to(&path, &s).unwrap();
        assert!(!fs::read_to_string(&path).unwrap().contains("sk-test"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn debug_masque_la_cle() {
        let text = format!("{:?}", sample());
        assert!(!text.contains("sk-test"));
        assert!(text.contains("***"));
    }

    #[test]
    fn cle_d_environnement_jamais_ecrite() {
        let env = Some("sk-env");
        assert!(key_is_from_env(" sk-env ", env));
        assert!(!key_is_from_env("autre", env));
        assert!(!key_is_from_env("", Some("")));
        assert_eq!(key_to_persist(true, "sk-env", env, None), None);
        assert_eq!(
            key_to_persist(true, "sk-env", env, Some("sk-old")),
            Some("sk-old".into())
        );
        assert_eq!(
            key_to_persist(true, "sk-mine", env, None),
            Some("sk-mine".into())
        );
        assert_eq!(key_to_persist(false, "sk-mine", env, Some("x")), None);
        assert_eq!(key_to_persist(true, "  ", None, None), None);
    }

    #[test]
    fn priorite_de_la_cle() {
        assert_eq!(initial_key(Some("env"), Some("saved")), "env");
        assert_eq!(initial_key(None, Some("saved")), "saved");
        assert_eq!(initial_key(Some("  "), Some("saved")), "saved");
        assert_eq!(initial_key(None, None), "");
    }

    #[test]
    fn chemin_xdg_puis_home() {
        let p = config_path_from(Some("/x/cfg"), Some("/home/u")).unwrap();
        assert_eq!(p, PathBuf::from("/x/cfg/quiz-rust/config.toml"));
        let p = config_path_from(None, Some("/home/u")).unwrap();
        assert_eq!(p, PathBuf::from("/home/u/.config/quiz-rust/config.toml"));
        assert_eq!(
            config_path_from(Some("relatif"), Some("/home/u")).unwrap(),
            PathBuf::from("/home/u/.config/quiz-rust/config.toml")
        );
        assert!(config_path_from(None, None).is_none());
    }
}
