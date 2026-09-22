# AGENTS.md

LazyWallet : TUI Rust (ratatui 0.26, crossterm 0.27, tokio, reqwest 0.11) qui affiche
une watchlist et des chandeliers depuis l'API chart de Yahoo Finance. Projet
d'apprentissage de Rust, développé en solo par Cyril.

Lire [`docs/architecture.md`](docs/architecture.md) avant de toucher au flux
thread principal ↔ worker, à `App` ou à `handle_event`. Lire
[`docs/candlestick-alignment.md`](docs/candlestick-alignment.md) avant de toucher au
rendu du graphique ou à l'axe X.

## Commandes

```bash
cargo build
cargo test                       # inclut un test réseau vers Yahoo (passe même hors ligne)
cargo clippy --all-targets -- -W clippy::pedantic
cargo fmt
cargo run                        # TUI plein écran : lancer dans un vrai terminal
```

Un changement est fini quand `cargo test`, `cargo clippy` et `cargo fmt --check`
passent. État au 2026-09-22 (`8878ec3`) : 2 tests en échec dans `models::ohlc`
(attentes de timeframes périmées), clippy pedantic ~265 warnings, et `cargo fmt`
jamais appliqué. Ce sont des dettes connues, pas des régressions ; ne pas les
mélanger à un autre changement.

## Conventions

- Commentaires et doc comments en **français**, style pédagogique (`// CONCEPT RUST : ...`) :
  c'est voulu, Cyril apprend Rust avec ce code. Garder ce ton sur le code modifié.
- README et messages de commit en anglais, Conventional Commits avec scope
  (`fix(ui): ...`, `feat(data): ...`).
- Erreurs : `anyhow::Result` + `.context(...)` ; les erreurs visibles par l'utilisateur
  doivent atteindre l'UI, pas seulement `tracing::error!`.
- Rendu du graphique : toute couche (chandeliers, ticks, labels) utilise les colonnes
  de `compute_candle_positions()`, jamais un calcul d'espacement local.

## Pièges

- `handle_event` (`src/main.rs`) est un seul `match` à gardes évalué dans l'ordre :
  un nouveau raccourci doit être gardé par l'écran (`app.is_on_dashboard()`, etc.),
  sinon il capture aussi les touches tapées en saisie.
- Les résultats du worker ciblent un item par **index** de watchlist ; toute opération
  qui réordonne ou supprime des items doit en tenir compte.
- `logs/` est relatif au répertoire de lancement et ignoré par git ; `RUST_LOG`
  surcharge le filtre `lazywallet=debug,info`.
- Le module `ui/chart.rs` et la struct `Ticker` sont du code mort : ne pas les
  étendre, le rendu actif est `ui/candlestick_text.rs`.
- `.specify/` est un gabarit spec-kit non rempli et non suivi par git ; il ne décrit
  pas le projet.
- Tester le rendu sans terminal : `ratatui::backend::TestBackend`, ou appeler
  `CandlestickRenderer::render_lines()` et inspecter les `Line`.
