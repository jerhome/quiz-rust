// Topic catalogue and the selection logic behind the config screen (no UI here).

use quiz_core::Lang;

use crate::i18n::Key;

pub const MAX_CUSTOM: usize = 20;

pub struct Theme {
    /// Stable identifier, persisted in config.toml (labels may change, ids must not).
    pub id: &'static str,
    pub fr: &'static str,
    pub en: &'static str,
}

impl Theme {
    pub fn label(&self, lang: Lang) -> &'static str {
        match lang {
            Lang::Fr => self.fr,
            Lang::En => self.en,
        }
    }
}

pub struct Category {
    pub title: Key,
    pub themes: &'static [Theme],
}

const fn th(id: &'static str, fr: &'static str, en: &'static str) -> Theme {
    Theme { id, fr, en }
}

pub const CATEGORIES: &[Category] = &[
    Category {
        title: Key::CatCulture,
        themes: &[
            th("general", "Culture générale", "General knowledge"),
            th("history", "Histoire", "History"),
            th("geography", "Géographie", "Geography"),
            th("flags", "Drapeaux & capitales", "Flags & capitals"),
            th(
                "monuments",
                "Monuments & architecture",
                "Landmarks & architecture",
            ),
            th("art", "Art & peinture", "Art & painting"),
            th("literature", "Littérature", "Literature"),
            th("mythology", "Mythologie", "Mythology"),
            th(
                "language",
                "Langue française & expressions",
                "Words & idioms",
            ),
            th(
                "religions",
                "Religions & philosophie",
                "Religions & philosophy",
            ),
        ],
    },
    Category {
        title: Key::CatScience,
        themes: &[
            th("science", "Sciences", "Science"),
            th("space", "Espace & astronomie", "Space & astronomy"),
            th("nature", "Nature & animaux", "Nature & animals"),
            th("body", "Corps humain & santé", "Human body & health"),
            th(
                "inventions",
                "Inventions & découvertes",
                "Inventions & discoveries",
            ),
            th(
                "tech",
                "Technologie & informatique",
                "Technology & computing",
            ),
            th("math", "Mathématiques & logique", "Mathematics & logic"),
            th("ecology", "Écologie & climat", "Ecology & climate"),
        ],
    },
    Category {
        title: Key::CatEntertainment,
        themes: &[
            th("cinema", "Cinéma", "Movies"),
            th("tv", "Séries TV", "TV series"),
            th("music", "Musique", "Music"),
            th("games", "Jeux vidéo", "Video games"),
            th("anime", "Mangas & animés", "Manga & anime"),
            th("cartoons", "Dessins animés", "Cartoons"),
            th("comics", "Bandes dessinées", "Comics"),
            th(
                "popculture",
                "Pop culture des années 80-90",
                "80s-90s pop culture",
            ),
        ],
    },
    Category {
        title: Key::CatSport,
        themes: &[
            th("sport", "Sport", "Sports"),
            th("football", "Football", "Football (soccer)"),
            th("food", "Gastronomie & cuisine", "Food & cooking"),
        ],
    },
    Category {
        title: Key::CatSociety,
        themes: &[
            th("celebrities", "Personnalités célèbres", "Famous people"),
            th("economy", "Économie & entreprises", "Economy & business"),
        ],
    },
];

pub fn all() -> impl Iterator<Item = &'static Theme> {
    CATEGORIES.iter().flat_map(|c| c.themes.iter())
}

pub fn find(id: &str) -> Option<&'static Theme> {
    all().find(|t| t.id == id)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
    Catalog(&'static str),
    Custom(String),
}

/// Lowercase and strip the accents used in French, so "ecologie" finds "Écologie".
pub fn normalize(s: &str) -> String {
    s.trim()
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'à' | 'â' | 'ä' => 'a',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

/// A catalogue theme matches a query in either language.
pub fn theme_matches(theme: &Theme, query: &str) -> bool {
    let q = normalize(query);
    q.is_empty() || normalize(theme.fr).contains(&q) || normalize(theme.en).contains(&q)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThemePicker {
    pub picks: Vec<Pick>,
    /// User-defined topics, most recent first.
    pub custom: Vec<String>,
    pub query: String,
}

impl ThemePicker {
    pub fn from_saved(ids: &[String], selected_custom: &[String], custom: &[String]) -> Self {
        let mut p = ThemePicker {
            custom: custom.iter().take(MAX_CUSTOM).cloned().collect(),
            ..Default::default()
        };
        for id in ids {
            if let Some(t) = find(id) {
                p.select(Pick::Catalog(t.id));
            }
        }
        for c in selected_custom {
            p.select(Pick::Custom(c.clone()));
        }
        p
    }

    pub fn catalog_ids(&self) -> Vec<String> {
        self.picks
            .iter()
            .filter_map(|p| match p {
                Pick::Catalog(id) => Some((*id).to_string()),
                Pick::Custom(_) => None,
            })
            .collect()
    }

    pub fn selected_custom(&self) -> Vec<String> {
        self.picks
            .iter()
            .filter_map(|p| match p {
                Pick::Custom(c) => Some(c.clone()),
                Pick::Catalog(_) => None,
            })
            .collect()
    }

    pub fn is_selected(&self, pick: &Pick) -> bool {
        self.picks.contains(pick)
    }

    fn select(&mut self, pick: Pick) {
        if !self.is_selected(&pick) {
            self.picks.push(pick);
        }
    }

    pub fn select_all(&mut self) {
        for th in all() {
            self.select(Pick::Catalog(th.id));
        }
        for c in self.custom.clone() {
            self.select(Pick::Custom(c));
        }
    }

    pub fn clear(&mut self) {
        self.picks.clear();
    }

    pub fn category_selected(&self, cat: &Category) -> bool {
        cat.themes
            .iter()
            .all(|th| self.is_selected(&Pick::Catalog(th.id)))
    }

    /// Selects the whole category, or deselects it if it is already fully selected.
    pub fn toggle_category(&mut self, cat: &Category) {
        if self.category_selected(cat) {
            self.picks.retain(
                |p| !matches!(p, Pick::Catalog(id) if cat.themes.iter().any(|t| t.id == *id)),
            );
        } else {
            for th in cat.themes {
                self.select(Pick::Catalog(th.id));
            }
        }
    }

    pub fn all_catalog_selected(&self) -> bool {
        all().all(|th| self.is_selected(&Pick::Catalog(th.id)))
    }

    pub fn toggle(&mut self, pick: Pick) {
        if let Some(i) = self.picks.iter().position(|p| *p == pick) {
            self.picks.remove(i);
        } else {
            self.select(pick);
        }
    }

    /// Remembers a free-form topic (case-insensitive dedupe, most recent first, capped)
    /// and selects it.
    pub fn add_custom(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let key = normalize(text);
        let existing = self.custom.iter().position(|c| normalize(c) == key);
        let name = match existing {
            Some(i) => self.custom.remove(i),
            None => text.to_string(),
        };
        self.custom.insert(0, name.clone());
        self.custom.truncate(MAX_CUSTOM);
        self.select(Pick::Custom(name));
    }

    pub fn remove_custom(&mut self, text: &str) {
        self.custom.retain(|c| c != text);
        self.picks.retain(|p| *p != Pick::Custom(text.to_string()));
    }

    pub fn filtered_custom(&self) -> Vec<&String> {
        let q = normalize(&self.query);
        self.custom
            .iter()
            .filter(|c| q.is_empty() || normalize(c).contains(&q))
            .collect()
    }

    /// Catalogue theme whose label equals the query exactly (either language).
    pub fn exact_match(&self) -> Option<&'static Theme> {
        let q = normalize(&self.query);
        all().find(|t| normalize(t.fr) == q || normalize(t.en) == q)
    }

    /// Nothing matches the typed text: offer to add it as a custom topic.
    pub fn can_add_query(&self) -> bool {
        let q = self.query.trim();
        !q.is_empty()
            && !all().any(|t| theme_matches(t, q))
            && !self.custom.iter().any(|c| normalize(c) == normalize(q))
    }

    /// Enter in the search box: pick an exact catalogue match, else add the text.
    pub fn submit_query(&mut self) {
        if let Some(t) = self.exact_match() {
            self.select(Pick::Catalog(t.id));
        } else {
            let q = self.query.clone();
            self.add_custom(&q);
        }
        self.query.clear();
    }

    pub fn label(&self, pick: &Pick, lang: Lang) -> String {
        match pick {
            Pick::Catalog(id) => {
                find(id).map_or_else(|| (*id).to_string(), |t| t.label(lang).into())
            }
            Pick::Custom(c) => c.clone(),
        }
    }

    /// Topic string sent to the model: catalogue labels in the *question* language,
    /// custom topics as typed; several topics get a balancing hint.
    pub fn topic_string(&self, lang: Lang) -> String {
        // Every catalogue topic and nothing else: one general instruction instead of a long list.
        if self.all_catalog_selected() && self.selected_custom().is_empty() {
            return match lang {
                Lang::Fr => "culture générale, toutes catégories confondues (mélange varié)".into(),
                Lang::En => "general knowledge across all categories (varied mix)".into(),
            };
        }
        let labels: Vec<String> = self.picks.iter().map(|p| self.label(p, lang)).collect();
        let joined = labels.join(", ");
        if labels.len() < 2 {
            return joined;
        }
        match lang {
            Lang::Fr => format!("mélange équilibré de : {joined}"),
            Lang::En => format!("an even mix of: {joined}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_coherent() {
        let n = all().count();
        assert!((30..=32).contains(&n), "{n}");
        let mut ids: Vec<_> = all().map(|t| t.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n);
        assert!(all().all(|t| !t.fr.trim().is_empty() && !t.en.trim().is_empty()));
        assert_eq!(CATEGORIES.len(), 5);
    }

    #[test]
    fn filtrage_sans_casse_ni_accents() {
        let eco = find("ecology").unwrap();
        assert!(theme_matches(eco, "ECOLOGIE"));
        assert!(theme_matches(eco, "climate"));
        assert!(!theme_matches(eco, "football"));
        assert!(theme_matches(eco, "  "));
        assert_eq!(normalize("Éléphant À Ça"), "elephant a ca");
    }

    #[test]
    fn ajout_perso() {
        let mut p = ThemePicker::default();
        p.add_custom("  Les chats ");
        p.add_custom("Volcans");
        p.add_custom("les CHATS");
        assert_eq!(p.custom, vec!["Les chats", "Volcans"]);
        assert_eq!(p.picks.len(), 2);
        for i in 0..30 {
            p.add_custom(&format!("t{i}"));
        }
        assert_eq!(p.custom.len(), MAX_CUSTOM);
        assert_eq!(p.custom[0], "t29");
        p.remove_custom("t29");
        assert!(!p.custom.contains(&"t29".to_string()));
        assert!(!p.is_selected(&Pick::Custom("t29".into())));
    }

    #[test]
    fn tout_selectionner_et_effacer() {
        let mut p = ThemePicker::default();
        p.add_custom("Volcans");
        p.clear();
        assert!(p.picks.is_empty());
        p.select_all();
        assert_eq!(p.picks.len(), all().count() + 1);
        assert!(p.all_catalog_selected());
        p.clear();
        assert!(p.picks.is_empty());
        assert_eq!(p.custom.len(), 1);
    }

    #[test]
    fn selection_de_categorie() {
        let mut p = ThemePicker::default();
        let cat = &CATEGORIES[3];
        p.toggle_category(cat);
        assert!(p.category_selected(cat));
        assert_eq!(p.picks.len(), cat.themes.len());
        p.toggle_category(cat);
        assert!(p.picks.is_empty());
    }

    #[test]
    fn chaine_tous_les_themes() {
        let mut p = ThemePicker::default();
        p.select_all();
        assert_eq!(
            p.topic_string(Lang::En),
            "general knowledge across all categories (varied mix)"
        );
        assert!(p
            .topic_string(Lang::Fr)
            .starts_with("culture générale, toutes catégories"));
        // with a custom topic the full list is sent, untruncated
        p.add_custom("Volcans");
        let s = p.topic_string(Lang::Fr);
        assert!(s.starts_with("mélange équilibré de : "));
        assert!(s.ends_with("Volcans"));
        assert_eq!(s.matches(", ").count(), all().count());
    }

    #[test]
    fn chaine_nombreux_themes() {
        let mut p = ThemePicker::default();
        for th in all().take(8) {
            p.toggle(Pick::Catalog(th.id));
        }
        let s = p.topic_string(Lang::En);
        assert_eq!(s.matches(", ").count(), 7);
        assert!(s.contains("Mythology"));
    }

    #[test]
    fn chaine_envoyee_au_modele() {
        let mut p = ThemePicker::default();
        assert_eq!(p.topic_string(Lang::Fr), "");
        p.toggle(Pick::Catalog("history"));
        assert_eq!(p.topic_string(Lang::Fr), "Histoire");
        assert_eq!(p.topic_string(Lang::En), "History");
        p.toggle(Pick::Catalog("cinema"));
        p.add_custom("Les volcans");
        assert_eq!(
            p.topic_string(Lang::Fr),
            "mélange équilibré de : Histoire, Cinéma, Les volcans"
        );
        assert_eq!(
            p.topic_string(Lang::En),
            "an even mix of: History, Movies, Les volcans"
        );
    }

    #[test]
    fn saisie_libre_et_entree() {
        let mut p = ThemePicker {
            query: "xyz".into(),
            ..Default::default()
        };
        assert!(p.can_add_query());
        p.query = "histoire".into();
        assert!(!p.can_add_query());
        p.submit_query();
        assert_eq!(p.picks, vec![Pick::Catalog("history")]);
        p.query = "Les abeilles".into();
        p.submit_query();
        assert_eq!(p.selected_custom(), vec!["Les abeilles"]);
        assert!(p.query.is_empty());
    }

    #[test]
    fn restauration_et_ids_inconnus() {
        let p = ThemePicker::from_saved(
            &["history".into(), "disparu".into()],
            &["Perso".into()],
            &["Perso".into(), "Autre".into()],
        );
        assert_eq!(p.catalog_ids(), vec!["history"]);
        assert_eq!(p.selected_custom(), vec!["Perso"]);
        assert_eq!(p.custom.len(), 2);
    }
}
