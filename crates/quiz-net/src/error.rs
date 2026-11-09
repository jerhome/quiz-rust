use crate::game::JoinError;
use crate::protocol::ErrorCode;
use quiz_core::Lang;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetError {
    #[error("Erreur réseau : {0}")]
    Io(#[from] std::io::Error),
    #[error("Délai dépassé : l'hôte ne répond pas.")]
    Timeout,
    #[error("Connexion refusée : {}", .0.message(Lang::Fr))]
    Rejected(ErrorCode),
    #[error("Réponse invalide de l'hôte : {0}")]
    Protocol(String),
    #[error("Nom invalide : {0}")]
    InvalidName(#[from] JoinError),
}

impl NetError {
    pub fn message(&self, lang: Lang) -> String {
        match (self, lang) {
            (NetError::Io(e), Lang::Fr) => format!("Erreur réseau : {e}"),
            (NetError::Io(e), Lang::En) => format!("Network error: {e}"),
            (NetError::Timeout, Lang::Fr) => "Délai dépassé : l'hôte ne répond pas.".into(),
            (NetError::Timeout, Lang::En) => "Timed out: the host is not responding.".into(),
            (NetError::Rejected(c), Lang::Fr) => format!("Connexion refusée : {}", c.message(lang)),
            (NetError::Rejected(c), Lang::En) => format!("Connection refused: {}", c.message(lang)),
            (NetError::Protocol(d), Lang::Fr) => format!("Réponse invalide de l'hôte : {d}"),
            (NetError::Protocol(d), Lang::En) => format!("Invalid response from the host: {d}"),
            (NetError::InvalidName(j), Lang::Fr) => format!("Nom invalide : {}", j.message(lang)),
            (NetError::InvalidName(j), Lang::En) => format!("Invalid name: {}", j.message(lang)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_has_both_messages() {
        let errs = [
            NetError::Io(std::io::Error::other("boom")),
            NetError::Timeout,
            NetError::Rejected(ErrorCode::NameTaken),
            NetError::Protocol("x".into()),
            NetError::InvalidName(JoinError::EmptyName),
        ];
        for e in &errs {
            let (fr, en) = (e.message(Lang::Fr), e.message(Lang::En));
            assert!(!fr.is_empty() && !en.is_empty() && fr != en, "{e:?}");
            assert_eq!(e.to_string(), fr);
        }
    }
}
