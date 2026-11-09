use quiz_core::Lang;

macro_rules! keys {
    ($($name:ident => ($fr:expr, $en:expr)),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Key { $($name),* }

        impl Key {
            #[allow(dead_code)]
            pub const ALL: &'static [Key] = &[$(Key::$name),*];

            fn pair(self) -> (&'static str, &'static str) {
                match self { $(Key::$name => ($fr, $en)),* }
            }
        }
    };
}

keys! {
    Tagline => ("Des quiz générés par l'IA, seul ou entre amis sur votre réseau local.",
                "AI-generated quizzes, solo or with friends on your local network."),
    SoloTitle => ("Solo", "Solo"),
    SoloDesc => ("Un thème, une difficulté, et c'est parti.", "Pick a topic and a difficulty, and go."),
    HostTitle => ("Héberger une partie", "Host a game"),
    HostDesc => ("Créez un quiz et invitez vos amis du réseau local.",
                 "Create a quiz and invite friends on your local network."),
    JoinTitle => ("Rejoindre une partie", "Join a game"),
    JoinDesc => ("Retrouvez une partie ouverte près de vous.", "Find an open game near you."),
    Settings => ("Réglages", "Settings"),
    NewQuizTitle => ("Nouveau quiz", "New quiz"),
    NewQuizSub => ("Choisissez un thème, l'IA s'occupe du reste.", "Pick a topic, the AI does the rest."),
    HostSetupTitle => ("Héberger une partie", "Host a game"),
    HostSetupSub => ("Choisissez le quiz, vos amis vous rejoindront.", "Choose the quiz, your friends will join you."),
    Topic => ("Thème", "Topic"),
    TopicHint => ("Ex. : l'histoire de Rome", "E.g. the history of Rome"),
    Difficulty => ("Difficulté", "Difficulty"),
    QuestionCount => ("Nombre de questions : {}", "Number of questions: {}"),
    QuestionLang => ("Langue des questions", "Question language"),
    YourName => ("Votre pseudo", "Your nickname"),
    NameHint => ("Ex. : Alice", "E.g. Alice"),
    TimePerQuestion => ("Temps par question", "Time per question"),
    Unlimited => ("Illimité", "Unlimited"),
    UnlimitedHint => ("Sans limite de temps : vous révélez la réponse vous-même.",
                      "No time limit: you reveal the answer yourself."),
    Seconds => ("{} s", "{} s"),
    ApiKeyLabel => ("Clé API Anthropic", "Anthropic API key"),
    Show => ("Afficher", "Show"),
    Hide => ("Masquer", "Hide"),
    Generate => ("Générer", "Generate"),
    Back => ("Retour", "Back"),
    ErrKeyRequired => ("La clé API est requise.", "An API key is required."),
    ErrNameRequired => ("Le pseudo est requis.", "A nickname is required."),
    ErrNameTooLong => ("Le pseudo est trop long ({} caractères max).", "The nickname is too long ({} characters max)."),
    ErrAddr => ("Adresse invalide : saisissez IP:port, par exemple 192.168.1.5:7878.",
                "Invalid address: enter IP:port, for example 192.168.1.5:7878."),
    ErrConfig => ("Configuration invalide : {}", "Invalid configuration: {}"),
    ErrHostStart => ("Impossible de démarrer l'hébergement : {}", "Could not start hosting: {}"),
    ErrDiscovery => ("Découverte automatique indisponible : {}", "Automatic discovery unavailable: {}"),
    ErrUnusable => ("Réponse inutilisable : {}", "Unusable response: {}"),
    ErrConnLost => ("La connexion avec l'hôte a été perdue.", "The connection with the host was lost."),
    ErrHost => ("Erreur de l'hôte : {}", "Host error: {}"),
    Generating => ("Génération des questions en cours…", "Generating questions…"),
    GeneratingHint => ("Cela ne devrait prendre que quelques secondes.", "This should only take a few seconds."),
    Opening => ("Ouverture de la partie…", "Opening the game…"),
    GenFailed => ("La génération a échoué", "Generation failed"),
    Retry => ("Réessayer", "Retry"),
    QuestionOf => ("Question {} / {}", "Question {} / {}"),
    GoodAnswer => ("Bonne réponse !", "Correct!"),
    BadAnswer => ("Mauvaise réponse", "Wrong answer"),
    Explanation => ("Explication", "Explanation"),
    Next => ("Suivant  (Entrée)", "Next  (Enter)"),
    QuizDone => ("Quiz terminé", "Quiz finished"),
    VerdictPerfect => ("Parfait !", "Perfect!"),
    VerdictGreat => ("Excellent travail", "Excellent work"),
    VerdictGood => ("Bien joué", "Well played"),
    VerdictOk => ("Pas mal, continuez", "Not bad, keep going"),
    VerdictLow => ("Courage, la prochaine sera la bonne", "Keep at it, the next one will be better"),
    PlayAgain => ("Rejouer", "Play again"),
    JoinHeading => ("Rejoindre une partie", "Join a game"),
    JoinSub => ("Choisissez un pseudo, puis une partie du réseau local.", "Pick a nickname, then a game on the local network."),
    NameHintJoin => ("Ex. : Bob", "E.g. Bob"),
    DetectedGames => ("Parties détectées", "Detected games"),
    Searching => ("Recherche en cours…", "Searching…"),
    ManualConnect => ("Connexion manuelle (IP:port)", "Manual connection (IP:port)"),
    Connect => ("Se connecter", "Connect"),
    Connecting => ("Connexion…", "Connecting…"),
    Lobby => ("Salon", "Lobby"),
    LobbySub => ("Les joueurs se rassemblent avant le début.", "Players gather before the game starts."),
    AddressToShare => ("Adresse à communiquer aux joueurs", "Address to share with players"),
    PlayersCount => ("Joueurs ({})", "Players ({})"),
    HostBadge => ("Hôte", "Host"),
    Launch => ("Lancer la partie", "Start the game"),
    ConfigureNext => ("Configurer une nouvelle partie", "Set up a new game"),
    Cancel => ("Annuler", "Cancel"),
    WaitingHost => ("En attente de l'hôte…", "Waiting for the host…"),
    MidGame => ("Partie en cours, en attente de la prochaine question…", "Game in progress, waiting for the next question…"),
    Leave => ("Quitter", "Leave"),
    AnswerSent => ("Réponse envoyée", "Answer sent"),
    AnswersReceived => ("Réponses reçues : {} / {}", "Answers received: {} / {}"),
    RevealNow => ("Révéler maintenant", "Reveal now"),
    RevealInProgress => ("Révélation en cours", "Reveal in progress"),
    Scores => ("Scores", "Scores"),
    You => ("vous", "you"),
    FinalRanking => ("Classement final", "Final ranking"),
    ThanksForPlaying => ("Merci d'avoir joué !", "Thanks for playing!"),
    Points => ("{} pt", "{} pts"),
    Tied => ("ex aequo", "tied"),
    NewGame => ("Nouvelle partie", "New game"),
    BackHome => ("Retour à l'accueil", "Back to home"),
    SettingsSub => ("Ces réglages sont enregistrés sur cet ordinateur.", "These settings are saved on this computer."),
    UiLanguage => ("Langue de l'interface", "Interface language"),
    Model => ("Modèle", "Model"),
    ModelOther => ("Autre…", "Other…"),
    ModelIdHint => ("Identifiant du modèle, ex. : claude-haiku-5-5", "Model identifier, e.g. claude-haiku-5-5"),
    RememberSaved => ("Mémoriser la clé API sur cet ordinateur", "Remember the API key on this computer"),
    RememberWarn => ("La clé sera enregistrée en clair dans un fichier réservé à votre utilisateur.",
                     "The key will be stored in plain text in a file readable only by your user."),
    EnvVarNotice => ("Clé fournie par la variable d'environnement ANTHROPIC_API_KEY : rien à mémoriser.",
                   "Key provided by the ANTHROPIC_API_KEY environment variable: nothing to remember."),
    Quit => ("Quitter", "Quit"),
    QuitTooltip => ("Quitter l'application (Ctrl+Q)", "Quit the application (Ctrl+Q)"),
    QuitTitle => ("Quitter l'application ?", "Quit the application?"),
    QuitInProgress => ("Le quiz en cours sera perdu.", "The quiz in progress will be lost."),
    QuitHost => ("Vous êtes l'hôte : la partie s'arrêtera pour tous les joueurs.",
                 "You are the host: the game will stop for all players."),
    QuitClient => ("Vous quitterez la partie en cours.", "You will leave the current game."),
    Display => ("Affichage", "Display"),
    DisplayHelp => ("X11 est recommandé, notamment sous WSL. Wayland peut offrir un rendu plus net sur un bureau Wayland natif.",
                    "X11 is recommended, especially on WSL. Wayland may look sharper on a native Wayland desktop."),
    DisplayCurrent => ("Actuellement : {}", "Currently: {}"),
    WaylandFallback => ("Wayland indisponible, X11 utilisé.", "Wayland unavailable, X11 in use."),
    RestartNeeded => ("Redémarrage nécessaire pour appliquer ce changement.", "A restart is needed to apply this change."),
    RestartNow => ("Redémarrer maintenant", "Restart now"),
    Later => ("Plus tard", "Later"),
    RestartTitle => ("Redémarrer l'application ?", "Restart the application?"),
    RestartConfirm => ("Redémarrer", "Restart"),
    RestartFailed => ("Impossible de redémarrer l'application.", "Could not restart the application."),
    CatCulture => ("Culture", "Culture"),
    CatScience => ("Sciences", "Science"),
    CatEntertainment => ("Divertissement", "Entertainment"),
    CatSport => ("Sport & loisirs", "Sports & leisure"),
    CatSociety => ("Société", "Society"),
    CatMine => ("Mes thèmes", "My topics"),
    TopicSearchHint => ("Rechercher ou saisir un thème…", "Search or type a topic…"),
    TopicAdd => ("+ Ajouter « {} »", "+ Add \"{}\""),
    TopicSelectAll => ("Tout sélectionner", "Select all"),
    TopicClear => ("Effacer", "Clear"),
    TopicRemove => ("Retirer de mes thèmes", "Remove from my topics"),
    TopicsSelected => ("{} thèmes sélectionnés", "{} topics selected"),
    TopicNone => ("Choisissez au moins un thème", "Pick at least one topic"),
    ForgetSaved => ("Oublier la clé enregistrée", "Forget the saved key"),
    SaveFailed => ("Enregistrement des réglages impossible.", "Could not save settings."),
}

pub fn t(lang: Lang, key: Key) -> &'static str {
    let (fr, en) = key.pair();
    match lang {
        Lang::Fr => fr,
        Lang::En => en,
    }
}

pub fn tf(lang: Lang, key: Key, args: &[&str]) -> String {
    let mut out = String::new();
    let mut args = args.iter();
    let mut rest = t(lang, key);
    while let Some(pos) = rest.find("{}") {
        out.push_str(&rest[..pos]);
        out.push_str(args.next().copied().unwrap_or(""));
        rest = &rest[pos + 2..];
    }
    out.push_str(rest);
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
    Key(Key),
    Pair { fr: String, en: String },
}

impl Msg {
    pub fn from_fn(f: impl Fn(Lang) -> String) -> Self {
        Msg::Pair {
            fr: f(Lang::Fr),
            en: f(Lang::En),
        }
    }

    pub fn text(&self, lang: Lang) -> String {
        match self {
            Msg::Key(k) => t(lang, *k).to_string(),
            Msg::Pair { fr, en } => match lang {
                Lang::Fr => fr.clone(),
                Lang::En => en.clone(),
            },
        }
    }
}

pub fn lang_from_locale(locale: &str) -> Lang {
    if locale.to_ascii_lowercase().starts_with("en") {
        Lang::En
    } else {
        Lang::Fr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toutes_les_cles_traduites() {
        for key in Key::ALL {
            for lang in Lang::ALL {
                assert!(!t(lang, *key).trim().is_empty(), "{key:?} / {lang:?}");
            }
        }
    }

    #[test]
    fn arguments_et_langues() {
        assert_eq!(
            tf(Lang::Fr, Key::QuestionOf, &["3", "10"]),
            "Question 3 / 10"
        );
        assert_eq!(tf(Lang::En, Key::PlayersCount, &["2"]), "Players (2)");
        assert_ne!(t(Lang::Fr, Key::Back), t(Lang::En, Key::Back));
    }

    #[test]
    fn nombre_d_arguments_coherent() {
        for key in Key::ALL {
            let (fr, en) = key.pair();
            assert_eq!(
                fr.matches("{}").count(),
                en.matches("{}").count(),
                "{key:?}"
            );
        }
    }

    #[test]
    fn message_bilingue() {
        let m = Msg::from_fn(|l| t(l, Key::Back).to_string());
        assert_eq!(m.text(Lang::Fr), "Retour");
        assert_eq!(m.text(Lang::En), "Back");
        assert_eq!(Msg::Key(Key::Retry).text(Lang::En), "Retry");
    }

    #[test]
    fn langue_depuis_locale() {
        assert_eq!(lang_from_locale("en_US.UTF-8"), Lang::En);
        assert_eq!(lang_from_locale("fr_FR.UTF-8"), Lang::Fr);
        assert_eq!(lang_from_locale(""), Lang::Fr);
        assert_eq!(lang_from_locale("de_DE"), Lang::Fr);
    }
}
