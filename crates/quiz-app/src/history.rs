// Remembers the prompts of questions already played so the generator can avoid repeating them.
// Stored as history.json next to config.toml; any read problem silently yields an empty history.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::settings;
use crate::themes::{normalize, Pick, ThemePicker};

pub const MAX_ENTRIES: usize = 500;
pub const AVOID_LIMIT: usize = 60;
const ALL_KEY: &str = "all";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    themes: Vec<String>,
    prompt: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct History {
    entries: Vec<Entry>,
}

pub fn pick_key(pick: &Pick) -> String {
    match pick {
        Pick::Catalog(id) => format!("cat:{id}"),
        Pick::Custom(c) => format!("custom:{}", normalize(c)),
    }
}

/// Language-independent keys of the current selection; `all` when the whole catalogue is chosen.
pub fn selection_keys(picker: &ThemePicker) -> Vec<String> {
    if picker.all_catalog_selected() && picker.selected_custom().is_empty() {
        return vec![ALL_KEY.to_string()];
    }
    picker.picks.iter().map(pick_key).collect()
}

pub fn path() -> Option<PathBuf> {
    settings::config_path().map(|p| p.with_file_name("history.json"))
}

impl History {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn record(&mut self, keys: &[String], prompts: impl IntoIterator<Item = String>) {
        for prompt in prompts {
            self.entries.retain(|e| e.prompt != prompt);
            self.entries.push(Entry {
                themes: keys.to_vec(),
                prompt,
            });
        }
        let excess = self.entries.len().saturating_sub(MAX_ENTRIES);
        self.entries.drain(..excess);
    }

    /// Most recent prompts for the selected themes, shared evenly between them (newest first
    /// within each theme, round-robin across themes), at most `limit` in total.
    pub fn avoid(&self, keys: &[String], limit: usize) -> Vec<String> {
        let per_theme: Vec<Vec<usize>> = keys
            .iter()
            .map(|k| {
                (0..self.entries.len())
                    .rev()
                    .filter(|&i| {
                        let t = &self.entries[i].themes;
                        k == ALL_KEY || t.iter().any(|x| x == k || x == ALL_KEY)
                    })
                    .collect()
            })
            .collect();
        let mut chosen: Vec<usize> = Vec::new();
        let mut round = 0;
        while chosen.len() < limit && per_theme.iter().any(|l| round < l.len()) {
            for list in &per_theme {
                if let Some(&i) = list.get(round) {
                    if chosen.len() < limit && !chosen.contains(&i) {
                        chosen.push(i);
                    }
                }
            }
            round += 1;
        }
        chosen
            .into_iter()
            .map(|i| self.entries[i].prompt.clone())
            .collect()
    }
}

pub fn load_from(path: &Path) -> History {
    fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<History>(&t).ok())
        .map(|mut h| {
            let excess = h.entries.len().saturating_sub(MAX_ENTRIES);
            h.entries.drain(..excess);
            h
        })
        .unwrap_or_default()
}

pub fn save_to(path: &Path, history: &History) -> std::io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::other("no parent directory"))?;
    settings::create_private_dir(dir)?;
    let text = serde_json::to_string(history).map_err(std::io::Error::other)?;
    let tmp = dir.join(format!(".history.json.{}.tmp", std::process::id()));
    let _ = fs::remove_file(&tmp);
    let result = (|| {
        let mut f = settings::create_private_file(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(k: &[&str]) -> Vec<String> {
        k.iter().map(|s| (*s).to_string()).collect()
    }

    fn fill(h: &mut History, key: &str, n: usize) {
        h.record(&keys(&[key]), (0..n).map(|i| format!("{key}-{i}")));
    }

    #[test]
    fn plus_recents_par_theme() {
        let mut h = History::default();
        fill(&mut h, "a", 100);
        fill(&mut h, "b", 10);
        let avoid = h.avoid(&keys(&["a"]), AVOID_LIMIT);
        assert_eq!(avoid.len(), 60);
        assert_eq!(avoid[0], "a-99");
        assert_eq!(avoid[59], "a-40");
    }

    #[test]
    fn partage_entre_themes() {
        let mut h = History::default();
        fill(&mut h, "a", 100);
        fill(&mut h, "b", 100);
        let avoid = h.avoid(&keys(&["a", "b"]), AVOID_LIMIT);
        assert_eq!(avoid.len(), 60);
        assert_eq!(avoid.iter().filter(|p| p.starts_with("a-")).count(), 30);
        assert_eq!(avoid.iter().filter(|p| p.starts_with("b-")).count(), 30);
        // a theme with little history leaves room for the other
        let mut h = History::default();
        fill(&mut h, "a", 100);
        fill(&mut h, "b", 5);
        let avoid = h.avoid(&keys(&["a", "b"]), AVOID_LIMIT);
        assert_eq!(avoid.iter().filter(|p| p.starts_with("b-")).count(), 5);
        assert_eq!(avoid.len(), 60);
    }

    #[test]
    fn plafond_500() {
        let mut h = History::default();
        fill(&mut h, "a", 520);
        assert_eq!(h.len(), MAX_ENTRIES);
        let all = h.avoid(&keys(&["a"]), 1000);
        assert_eq!(all.len(), MAX_ENTRIES);
        assert!(!all.contains(&"a-19".to_string()));
        assert!(all.contains(&"a-20".to_string()));
    }

    #[test]
    fn doublon_remonte_et_theme_all() {
        let mut h = History::default();
        h.record(&keys(&["a"]), ["q1".to_string(), "q2".to_string()]);
        h.record(&keys(&["a"]), ["q1".to_string()]);
        assert_eq!(h.len(), 2);
        assert_eq!(h.avoid(&keys(&["a"]), 10), ["q1", "q2"]);
        h.record(&keys(&[ALL_KEY]), ["m".to_string()]);
        assert!(h.avoid(&keys(&["a"]), 10).contains(&"m".to_string()));
        assert_eq!(h.avoid(&keys(&[ALL_KEY]), 10).len(), 3);
        assert!(h.avoid(&[], 10).is_empty());
    }

    #[test]
    fn cle_de_theme_normalisee() {
        assert_eq!(pick_key(&Pick::Catalog("history")), "cat:history");
        assert_eq!(
            pick_key(&Pick::Custom("  Écologie ".into())),
            pick_key(&Pick::Custom("ecologie".into()))
        );
        let mut p = ThemePicker::default();
        p.select_all();
        assert_eq!(selection_keys(&p), ["all"]);
        p.clear();
        p.toggle(Pick::Catalog("history"));
        assert_eq!(selection_keys(&p), ["cat:history"]);
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("quiz-hist-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d.join("history.json")
    }

    #[test]
    fn fichier_corrompu_ou_absent() {
        let p = tmp("bad");
        assert_eq!(load_from(&p), History::default());
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, "{ pas du json").unwrap();
        assert_eq!(load_from(&p), History::default());
        let _ = fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn aller_retour() {
        let p = tmp("rt");
        let mut h = History::default();
        fill(&mut h, "a", 3);
        save_to(&p, &h).unwrap();
        assert_eq!(load_from(&p), h);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        let _ = fs::remove_dir_all(p.parent().unwrap());
    }
}
