use quiz_core::{Question, CHOICES_PER_QUESTION};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;

#[derive(Deserialize)]
struct RawQuestion {
    prompt: String,
    choices: Vec<String>,
    correct: usize,
    explanation: String,
}

fn validate(v: Value) -> Option<Question> {
    let r: RawQuestion = serde_json::from_value(v).ok()?;
    let prompt = r.prompt.trim().to_string();
    let explanation = r.explanation.trim().to_string();
    let choices: Vec<String> = r.choices.iter().map(|c| c.trim().to_string()).collect();
    if prompt.is_empty() || explanation.is_empty() {
        return None;
    }
    if choices.len() != CHOICES_PER_QUESTION || r.correct >= CHOICES_PER_QUESTION {
        return None;
    }
    if choices.iter().any(String::is_empty) {
        return None;
    }
    // Case-insensitive: "Paris" and "paris" would be indistinguishable choices.
    let distinct: HashSet<String> = choices.iter().map(|c| c.to_lowercase()).collect();
    if distinct.len() != CHOICES_PER_QUESTION {
        return None;
    }
    Some(Question {
        id: 0,
        prompt,
        choices,
        correct: r.correct,
        explanation,
    })
}

/// Invalid questions are silently dropped; the caller decides whether enough remain.
pub(crate) fn filter_valid(raw: Vec<Value>) -> Vec<Question> {
    raw.into_iter().filter_map(validate).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn good() -> Value {
        json!({"prompt":"P","choices":["a","b","c","d"],"correct":2,"explanation":"E"})
    }

    #[test]
    fn accepts_valid() {
        let q = validate(good()).unwrap();
        assert_eq!(q.correct, 2);
    }

    #[test]
    fn rejects_each_rule() {
        let mut cases = vec![];
        let mut v = good();
        v["prompt"] = json!("  ");
        cases.push(v);
        let mut v = good();
        v["explanation"] = json!("");
        cases.push(v);
        let mut v = good();
        v["choices"] = json!(["a", "b", "c"]);
        cases.push(v);
        let mut v = good();
        v["choices"] = json!(["a", "b", "c", "d", "e"]);
        cases.push(v);
        let mut v = good();
        v["choices"] = json!(["a", "b", "", "d"]);
        cases.push(v);
        let mut v = good();
        v["choices"] = json!(["a", "b", "A", "d"]);
        cases.push(v);
        let mut v = good();
        v["correct"] = json!(4);
        cases.push(v);
        let mut v = good();
        v["correct"] = json!(-1);
        cases.push(v);
        cases.push(json!({"prompt":"P"}));
        cases.push(json!("not an object"));
        for c in cases {
            assert!(validate(c.clone()).is_none(), "devrait échouer : {c}");
        }
    }
}
