use quiz_core::{Lang, Question};
use std::collections::HashSet;

pub(crate) const MAX_AVOID: usize = 60;
pub(crate) const MAX_AVOID_CHARS: usize = 140;

/// Hints that keep repeated quizzes on the same theme from converging on the same facts.
#[derive(Debug, Clone, Default)]
pub struct Variety {
    pub avoid: Vec<String>,
    pub seed: Option<u64>,
}

/// `(French, English)` focus angles.
const ANGLES: &[(&str, &str)] = &[
    ("les personnes", "people"),
    ("les lieux", "places"),
    ("les dates et les époques", "dates and eras"),
    ("les chiffres et les records", "figures and records"),
    ("les origines et l'étymologie", "origins and etymology"),
    ("les anecdotes peu connues", "little-known anecdotes"),
    ("les œuvres et les inventions", "works and inventions"),
    ("les idées reçues", "common misconceptions"),
    ("les comparaisons", "comparisons"),
    ("la vie quotidienne", "everyday life"),
];

pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed)
    }

    // splitmix64
    pub(crate) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

pub(crate) fn resolve_seed(v: &Variety) -> u64 {
    if let Some(s) = v.seed {
        return s;
    }
    let mut b = [0u8; 8];
    if getrandom::fill(&mut b).is_ok() {
        return u64::from_le_bytes(b);
    }
    // Fallback: per-process random keys from the std hasher.
    use std::hash::{BuildHasher, Hasher};
    std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish()
}

pub(crate) fn pick_angles(seed: u64, lang: Lang) -> [&'static str; 2] {
    let mut rng = Rng::new(seed);
    let a = rng.below(ANGLES.len());
    let mut b = rng.below(ANGLES.len() - 1);
    if b >= a {
        b += 1;
    }
    let pick = |i: usize| match lang {
        Lang::Fr => ANGLES[i].0,
        Lang::En => ANGLES[i].1,
    };
    [pick(a), pick(b)]
}

/// Bounded, single-line entries safe to embed in the prompt.
pub(crate) fn avoid_entries(avoid: &[String]) -> Vec<String> {
    avoid
        .iter()
        .map(|s| {
            s.split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(MAX_AVOID_CHARS)
                .collect::<String>()
        })
        .filter(|s| !s.is_empty())
        .take(MAX_AVOID)
        .collect()
}

fn fold(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        c => c,
    }
}

pub(crate) fn normalize(s: &str) -> String {
    let folded: String = s
        .to_lowercase()
        .chars()
        .map(fold)
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    folded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Drops questions whose normalized prompt is in `avoid` or already seen in `seen`.
pub(crate) fn drop_duplicates(
    qs: Vec<Question>,
    avoid: &[String],
    seen: &mut HashSet<String>,
) -> Vec<Question> {
    for a in avoid {
        seen.insert(normalize(a));
    }
    qs.into_iter()
        .filter(|q| seen.insert(normalize(&q.prompt)))
        .collect()
}

pub(crate) fn shuffle_choices(q: &mut Question, rng: &mut Rng) {
    let correct = q.choices[q.correct].clone();
    for i in (1..q.choices.len()).rev() {
        q.choices.swap(i, rng.below(i + 1));
    }
    if let Some(p) = q.choices.iter().position(|c| *c == correct) {
        q.correct = p;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(prompt: &str) -> Question {
        Question {
            id: 0,
            prompt: prompt.into(),
            choices: ["a", "b", "c", "d"].map(String::from).to_vec(),
            correct: 0,
            explanation: "e".into(),
        }
    }

    #[test]
    fn normalize_strips_case_accents_punctuation_spaces() {
        assert_eq!(
            normalize("  Quelle est   l'ÉPOQUE ?!"),
            "quelle est l epoque"
        );
    }

    #[test]
    fn angles_are_two_distinct_and_seed_dependent() {
        let mut firsts = HashSet::new();
        for s in 0..50 {
            let [a, b] = pick_angles(s, Lang::En);
            assert_ne!(a, b);
            firsts.insert((a, b));
            assert_eq!(pick_angles(s, Lang::En), [a, b]);
        }
        assert!(firsts.len() > 5);
    }

    #[test]
    fn avoid_is_limited_truncated_single_line() {
        let long = "x".repeat(500);
        let mut v: Vec<String> = (0..100).map(|i| format!("q{i}\nline")).collect();
        v.insert(0, long);
        v.insert(1, "   ".into());
        let e = avoid_entries(&v);
        assert_eq!(e.len(), MAX_AVOID);
        assert_eq!(e[0].chars().count(), MAX_AVOID_CHARS);
        assert!(e.iter().all(|s| !s.contains('\n')));
        assert_eq!(e[1], "q0 line");
    }

    #[test]
    fn duplicates_dropped_vs_avoid_and_within_batch() {
        let avoid = vec!["Quelle est la capitale ?".to_string()];
        let out = drop_duplicates(
            vec![
                q("quelle est la capitale"),
                q("Autre"),
                q("AUTRE !"),
                q("Encore"),
            ],
            &avoid,
            &mut HashSet::new(),
        );
        let p: Vec<_> = out.iter().map(|q| q.prompt.as_str()).collect();
        assert_eq!(p, ["Autre", "Encore"]);
    }

    #[test]
    fn shuffle_keeps_correct_answer_and_varies_position() {
        let mut positions = HashSet::new();
        for seed in 0..40 {
            let mut x = q("p");
            x.correct = 2;
            let before = x.choices[2].clone();
            let mut sorted = x.choices.clone();
            shuffle_choices(&mut x, &mut Rng::new(seed));
            assert_eq!(x.choices[x.correct], before);
            sorted.sort();
            let mut after = x.choices.clone();
            after.sort();
            assert_eq!(sorted, after);
            positions.insert(x.correct);
        }
        assert_eq!(positions.len(), 4);
    }
}
