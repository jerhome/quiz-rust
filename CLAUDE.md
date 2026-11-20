# Quiz Rust — Instructions projet

Application de quiz de bureau en Rust. Les questions/réponses sont générées par l'API Claude
à partir d'un thème choisi par le joueur. Phase 1 : solo. Phase 2 : multijoueur sur réseau local (LAN).

Conventions communes de mes projets Rust : `../agents-home/docs/rust/CONVENTIONS.md` (Home Repo `../agents-home`,
lis son `AGENTS.md`). En cas de conflit, ce fichier et les ADR du repo priment.

## Contraintes non négociables

- **Cible** : Linux, fenêtrage **X11 par défaut** (tourne sous WSL2 via WSLg / XWayland) ; **Wayland** activable dans les Réglages (pris en compte au redémarrage, repli automatique sur X11 si Wayland est indisponible). Cible secondaire : **Windows 11** (`x86_64-pc-windows-gnu`, compilé depuis WSL avec mingw-w64) ; le code spécifique à un OS passe par `#[cfg(unix)]` / `#[cfg(windows)]`, config sous `%APPDATA%\quiz-rust\config.toml`.
- **Langage** : Rust stable, édition 2021, aucun `unsafe`.
- **Clé API** : jamais en dur, jamais loggée, jamais commitée. Lue dans `ANTHROPIC_API_KEY`,
  sinon saisie dans l'UI. Si l'utilisateur choisit de la sauvegarder : `~/.config/quiz-rust/config.toml`
  avec permissions `600`. `config.toml` et `.env` sont dans `.gitignore`.
- **Pas d'OpenSSL** : utiliser `rustls` partout (simplifie la compilation).
- Économie de tokens : lire uniquement les fichiers nécessaires, pas de relecture globale du dépôt.
- **Commentaires** : en anglais uniquement, et seulement quand un humain pourrait ne pas comprendre le code sans aide (invariant non évident, raison d'un choix, piège). Pas de commentaire qui paraphrase le code, pas de `///` systématique sur l'API publique. Les textes affichés à l'utilisateur restent traduits via i18n.

## Stack

| Rôle | Crate |
|---|---|
| GUI | `eframe` / `egui`, `default-features = false`, features `["default_fonts", "glow", "x11", "wayland"]` ; backend choisi au démarrage (X11 par défaut) |
| Async | `tokio` (rt-multi-thread, net, macros, sync, time) |
| HTTP | `reqwest` avec `default-features = false`, features `["json", "rustls-tls"]` |
| Sérialisation | `serde`, `serde_json`, `toml` |
| Erreurs | Types d'erreur typés dans les libs (`thiserror` de préférence ; `Display` manuel accepté), messages UI via `message(lang: Lang)` ; `anyhow` dans le binaire |
| Logs | `tracing`, `tracing-subscriber` |
| Tests HTTP | `wiremock` (dev-dependency) |

## Architecture (workspace Cargo)

```text
quiz-rust/
├── Cargo.toml              # [workspace]
├── crates/
│   ├── quiz-core/          # Domaine pur : Question, Quiz, Score, machine à états. AUCUNE I/O.
│   ├── quiz-llm/           # Client API Claude + génération de questions
│   ├── quiz-net/           # Phase 2 : protocole LAN (serveur/hôte + client + découverte)
│   └── quiz-app/           # Binaire eframe : écrans, config, orchestration
└── .claude/agents/         # core-dev, ui-dev, qa-tester, triage
```

Règle de dépendances : `quiz-app → quiz-llm, quiz-net → quiz-core`. `quiz-core` ne dépend d'aucun autre crate du workspace.

## Modèle de domaine (quiz-core)

```rust
pub struct Question {
    pub id: u32,
    pub prompt: String,
    pub choices: Vec<String>,   // 4 choix
    pub correct: usize,         // index dans choices
    pub explanation: String,
}
pub struct QuizConfig { pub theme: String, pub difficulty: Difficulty, pub count: u8 /* 5..=20 */, pub language: String }
pub enum Difficulty { Facile, Moyen, Difficile }
pub enum QuizState { Setup, Generating, Question { index: usize }, Reveal { index: usize }, Finished }
```

La machine à états est la seule source de vérité ; l'UI ne fait que l'afficher et lui envoyer des actions.

## Génération via l'API Claude (quiz-llm)

- `POST https://api.anthropic.com/v1/messages`, en-têtes `x-api-key`, `anthropic-version: 2023-06-01`.
- Modèle configurable, défaut `claude-haiku-5-5` (rapide, peu coûteux).
- Sortie structurée via un **outil forcé** (`tool_choice: {"type":"tool","name":"submit_quiz"}`)
  dont l'`input_schema` JSON décrit la liste de questions. Ne jamais parser du texte libre.
- Valider chaque question après réception : 4 choix distincts, `correct` dans les bornes, texte non vide.
  Questions invalides écartées ; si trop peu restent → une seule nouvelle tentative, puis erreur claire.
- Timeout 60 s, erreurs typées (`AuthError`, `RateLimited`, `Network`, `InvalidResponse`) affichées
  en français dans l'UI.
- Les appels réseau tournent sur le runtime tokio ; l'UI reçoit le résultat par canal
  (`tokio::sync::mpsc` ou `std::sync::mpsc`) et appelle `ctx.request_repaint()`. Jamais de blocage du thread UI.

## Phase 2 — Multijoueur LAN (quiz-net)

- L'**hôte** génère le quiz (lui seul a besoin d'une clé API) et fait autorité sur le score et le temps.
- Transport : TCP, port par défaut `7878`, messages JSON une ligne par message (`\n`).
- Découverte : broadcast UDP sur `7879` (annonce `{"game":"quiz-rust","name":..,"port":..}` toutes les 2 s).
- Messages : `enum ClientMsg { Join{name}, Answer{question_id, choice} }`,
  `enum ServerMsg { Welcome, Lobby{players}, Question{..sans la bonne réponse..}, Reveal{correct, scores}, End{ranking} }`.
- **Anti-triche** : la bonne réponse n'est jamais envoyée avant `Reveal`.
- Version de protocole dans `Welcome` ; refuser un client de version différente.
- **Joueurs web** (v0.3) : l'hôte sert aussi la page joueur mobile (`crates/quiz-app/web/`, embarquée) et une
  WebSocket `/ws` sur le port `7880` ; mêmes messages JSON (un par frame texte), même heartbeat Ping/Pong.
  La page est autonome : aucun CDN ni Google Fonts (CSP stricte côté serveur), textes réseau insérés via `textContent`.
- **Reprise de session** : `Welcome` donne un `resume_token`, renvoyé dans `Join.resume` ; un joueur coupé reste
  « déconnecté » (score gardé, non attendu pour la révélation) pendant 10 min avant d'être retiré.

## Commandes

```bash
# Dépendances système (Ubuntu/Debian, WSL2)
sudo apt install build-essential pkg-config libx11-dev libxcursor-dev libxrandr-dev \
  libxi-dev libxkbcommon-dev libxkbcommon-x11-dev libgl1-mesa-dev

cargo build --workspace
cargo run -p quiz-app
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

Le backend (X11/Wayland) se choisit dans les Réglages ; X11 est forcé par défaut, même si `WAYLAND_DISPLAY` est défini.

## Définition de « terminé »

Une tâche est terminée quand `fmt`, `clippy -D warnings` et `test` passent sur tout le workspace,
et qu'aucun test n'appelle la vraie API (mock `wiremock` uniquement).

## Répartition des agents

| Agent | Périmètre | Ne touche pas |
|---|---|---|
| `core-dev` | `quiz-core`, `quiz-llm`, `quiz-net` | `quiz-app` (sauf signatures publiques à coordonner) |
| `ui-dev` | `quiz-app` (dont la page web `crates/quiz-app/web/`) | logique métier, protocole |
| `qa-tester` | tests, CI locale, revue | code de production (signale, ne corrige pas) |
| `triage` | diagnostic des échecs et routage | tout fichier (lecture seule) |

Ordre type d'une fonctionnalité : `core-dev` (API publique + tests unitaires) → `ui-dev` (écran) → `qa-tester` (vérif complète).
Les agents communiquent via des résumés courts : fichiers modifiés, API publique ajoutée, points ouverts.

### Boucle d'échec (orchestrée par la session principale)

Un sous-agent ne peut pas en lancer un autre : c'est la session principale qui enchaîne les étapes.

1. `qa-tester` rend `VERDICT : À CORRIGER` (ou une commande échoue) → lancer `triage` avec la sortie d'erreur.
2. `triage` rend un rapport « → core-dev / → ui-dev / → environnement » et un `ORDRE`.
3. Lancer les agents désignés dans cet ordre (en parallèle seulement si `triage` dit « indépendants »),
   en leur donnant uniquement leurs lignes du rapport.
4. Relancer `qa-tester`. Maximum **3 cycles** sur le même échec, puis s'arrêter et demander à l'utilisateur.
5. Les points « environnement / utilisateur » ne sont confiés à aucun agent : les remonter tels quels.

## Feuille de route

1. Squelette workspace + fenêtre eframe vide qui s'ouvre sous X11.
2. `quiz-core` : modèle + machine à états + tests.
3. `quiz-llm` : client + génération + tests wiremock.
4. `quiz-app` : écrans Config → Génération → Question → Révélation → Score.
5. Persistance optionnelle de la config (clé, modèle, langue).
6. Phase 2 : `quiz-net` (hôte, client, découverte) puis écrans Lobby/Rejoindre.
