use quiz_core::Lang;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
/// `Display` is French; use `message(lang)` for the UI language. Technical details
/// (`Network`, `InvalidResponse`, `Api.message`) are untranslated diagnostics.
pub enum LlmError {
    AuthError,
    RateLimited,
    Network(String),
    InvalidResponse(String),
    Api {
        status: u16,
        kind: Option<String>,
        message: Option<String>,
    },
    TooFewQuestions {
        valid: usize,
        expected: usize,
    },
    InvalidModel,
}

impl LlmError {
    pub fn message(&self, lang: Lang) -> String {
        let fr = lang == Lang::Fr;
        match self {
            LlmError::AuthError if fr => {
                "Clé API invalide ou non autorisée. Vérifiez votre clé.".into()
            }
            LlmError::AuthError => "Invalid or unauthorized API key. Check your key.".into(),
            LlmError::RateLimited if fr => {
                "Trop de requêtes ou quota dépassé. Réessayez dans quelques instants.".into()
            }
            LlmError::RateLimited => "Too many requests or quota exceeded. Try again shortly.".into(),
            LlmError::Network(d) if fr => format!("Erreur réseau : {d}"),
            LlmError::Network(d) => format!("Network error: {d}"),
            LlmError::InvalidResponse(d) if fr => format!("Réponse inattendue de l'API : {d}"),
            LlmError::InvalidResponse(d) => format!("Unexpected API response: {d}"),
            LlmError::Api {
                status,
                kind,
                message,
            } => api_message(lang, *status, kind.as_deref(), message.as_deref()),
            LlmError::TooFewQuestions { valid, expected } if fr => format!(
                "Seulement {valid} question(s) valide(s) sur {expected} demandées, malgré une nouvelle tentative."
            ),
            LlmError::TooFewQuestions { valid, expected } => format!(
                "Only {valid} valid question(s) out of {expected} requested, even after a retry."
            ),
            LlmError::InvalidModel if fr => "Identifiant de modèle invalide ou vide.".into(),
            LlmError::InvalidModel => "Invalid or empty model identifier.".into(),
        }
    }

    pub fn message_fr(&self) -> String {
        self.message(Lang::Fr)
    }
}

impl fmt::Display for LlmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message(Lang::Fr))
    }
}

impl std::error::Error for LlmError {}

fn api_message(lang: Lang, status: u16, kind: Option<&str>, message: Option<&str>) -> String {
    let fr = lang == Lang::Fr;
    let msg = message.unwrap_or("");
    if msg.to_lowercase().contains("credit balance") {
        return if fr {
            "Crédit API insuffisant : rechargez votre compte Anthropic.".into()
        } else {
            "Insufficient API credit: top up your Anthropic account.".into()
        };
    }
    if kind == Some("not_found_error") || status == 404 {
        return if fr {
            "Modèle introuvable : vérifiez le nom du modèle configuré.".into()
        } else {
            "Model not found: check the configured model name.".into()
        };
    }
    let mut s = if fr {
        format!("Requête refusée par l'API (HTTP {status}")
    } else {
        format!("Request rejected by the API (HTTP {status}")
    };
    if let Some(k) = kind {
        s.push_str(&format!(", {k}"));
    }
    s.push(')');
    if !msg.is_empty() {
        s.push_str(&format!(": {msg}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_has_both_messages() {
        let errs = [
            LlmError::AuthError,
            LlmError::RateLimited,
            LlmError::Network("x".into()),
            LlmError::InvalidResponse("x".into()),
            LlmError::Api {
                status: 400,
                kind: Some("invalid_request_error".into()),
                message: Some("boom".into()),
            },
            LlmError::Api {
                status: 400,
                kind: None,
                message: Some("Your credit balance is too low".into()),
            },
            LlmError::Api {
                status: 404,
                kind: Some("not_found_error".into()),
                message: None,
            },
            LlmError::TooFewQuestions {
                valid: 1,
                expected: 5,
            },
            LlmError::InvalidModel,
        ];
        for e in &errs {
            let (fr, en) = (e.message(Lang::Fr), e.message(Lang::En));
            assert!(!fr.is_empty() && !en.is_empty());
            assert_ne!(fr, en, "{e:?}");
            assert_eq!(e.to_string(), fr);
        }
    }

    #[test]
    fn english_special_cases() {
        let credit = LlmError::Api {
            status: 400,
            kind: None,
            message: Some("credit balance too low".into()),
        };
        assert!(credit.message(Lang::En).contains("credit"));
        let nf = LlmError::Api {
            status: 404,
            kind: None,
            message: None,
        };
        assert!(nf.message(Lang::En).contains("not found"));
    }
}
