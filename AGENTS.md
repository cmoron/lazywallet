# AGENTS.md

LazyWallet : TUI Rust (ratatui 0.26, crossterm 0.27, tokio, reqwest 0.11) qui affiche
une watchlist et des chandeliers depuis l'API chart de Yahoo Finance. Projet
d'apprentissage de Rust, développé en solo par Cyril.

Lire [`docs/architecture.md`](docs/architecture.md) avant de toucher au flux
thread principal ↔ worker, à `App` ou à `handle_key`. Lire
[`docs/chart-rendering.md`](docs/chart-rendering.md) avant de toucher au graphique
ou à ses axes.

## Commandes

```bash
cargo test                                  # hors ligne
cargo test -- --ignored                     # + le test qui appelle Yahoo
cargo clippy --all-targets -- -D warnings   # pedantic, réglé dans Cargo.toml
cargo fmt
cargo run                                   # TUI plein écran
```

Un changement est fini quand `cargo test`, `cargo clippy --all-targets -- -D warnings`
et `cargo fmt --check` passent.

## Conventions

- Commentaires et doc comments en **français**, style pédagogique (`// CONCEPT RUST : ...`) :
  c'est voulu, Cyril apprend Rust avec ce code. Garder ce ton sur le code modifié.
- README et messages de commit en anglais, Conventional Commits avec scope
  (`fix(ui): ...`, `feat(chart): ...`).
- Erreurs : `anyhow::Result` + `.context(...)` ; les erreurs visibles par l'utilisateur
  passent par `App::set_error`, pas seulement `tracing::error!`. Les messages
  d'erreur Yahoo commencent par le symbole.
- Les transitions de `App` qui ont besoin du réseau retournent des `AppCommand` ;
  seul `main.rs` les envoie au worker.

## Pièges

- Le worker renvoie **exactement un** `AppResult` par `AppCommand` : `App::pending`
  (indicateur de chargement, blocage du refresh auto) en dépend.
- `handle_key` route d'abord par écran : en `InputMode`, toute touche va à la
  saisie. Un nouveau raccourci se place dans la branche de son écran.
- `CandleChart` ne dessine que dans le `Rect` reçu ; l'écran lui passe
  `block.inner(area)`. Toute couche du graphique réutilise les colonnes de
  `visible_columns`.
- Les heures du graphique sont celles de la place (`OHLCData::local_time`), pas UTC
  ni l'heure locale de la machine.
- `logs/` est relatif au répertoire de lancement et ignoré par git ; `RUST_LOG`
  surcharge le filtre `lazywallet=debug,info`.
- Tester un rendu sans terminal : `ratatui::backend::TestBackend`, ou rendre le
  widget dans un `Buffer::empty(area)` et lire les cellules.
- Piloter le TUI réel dans tmux : toujours un serveur dédié (`tmux -L lw ...`,
  `tmux -L lw kill-server`). Un `tmux kill-server` nu tue aussi la session de
  l'agent.
