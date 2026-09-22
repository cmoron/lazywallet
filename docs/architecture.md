# Architecture de LazyWallet

> Décrit le code de la branche `review-fixes` (2026-09-23).
> Les évolutions envisagées sont dans la Roadmap du README, pas ici.

## Vue d'ensemble

TUI Rust (ratatui 0.26 + crossterm 0.27) qui affiche une watchlist et un graphique en
chandeliers pour le ticker sélectionné. Source unique de données : l'endpoint
`v8/finance/chart` de Yahoo Finance, sans clé ni cookie.

Deux threads, aucun état partagé :

- **Thread principal** : seul propriétaire de `App`. Event loop, rendu, clavier.
- **Worker** (`worker.rs`, runtime tokio `current_thread`) : transforme chaque
  `AppCommand` en exactement un `AppResult`. Il ne touche jamais à `App`.

```
            AppCommand (Load / Add)
  main ────────────────────────────────▶ worker ──▶ Yahoo
   ▲  App::pending += 1                    │
   └───────────────────────────────────────┘
            AppResult (Loaded / Added / Failed), App::pending -= 1
```

## Modules

```
src/
├── main.rs            # logging, panic hook, terminal, event loop
├── lib.rs             # expose les modules (tests, binaire)
├── app.rs             # App (état) + transitions qui retournent des AppCommand
├── worker.rs          # AppCommand, AppResult, spawn_worker
├── watchlist_file.rs  # ~/.config/lazywallet/watchlist.txt
├── api/yahoo.rs       # http_client, fetch_ticker_data, parse (testé sur JSON)
├── models/
│   ├── ohlc.rs            # Interval, OHLC, OHLCData (utc_offset, séances)
│   └── watchlist_item.rs  # WatchlistItem, Quote, FetchedTicker
└── ui/
    ├── events.rs      # EventHandler (poll 250 ms) + handle_key
    ├── dashboard.rs   # routage par écran, watchlist, status_line, saisie
    └── chart/         # écran graphique + widget (voir chart-rendering.md)
```

### `main.rs`

- `init_logging()` : `tracing` vers `./logs/lazywallet.log.YYYY-MM-DD` (relatif au
  répertoire de lancement). Filtre `lazywallet=debug,info`, surchargé par `RUST_LOG`.
- `install_panic_hook()` : restaure le terminal avant d'afficher un panic.
- `run()`, à chaque tour :
  1. vide **tous** les résultats arrivés (`apply_result`) ;
  2. dessine ;
  3. `app.tick(now)` (expiration du statut, refresh auto) + `handle_key` si une
     touche arrive (attente max 250 ms) ;
  4. `send_all` envoie les commandes et incrémente `pending`.
     Une erreur de lecture clavier ou un worker disparu termine la boucle avec une
     erreur, jamais en silence.

### `app.rs`

`App` porte tout l'état : `watchlist`, `selected_index`, `current_screen`
(`Dashboard | ChartView | InputMode`), `current_interval` (commun à tous les
tickers), `confirm_quit` / `confirm_delete`, `pending` (commandes en vol),
`status` (message info/erreur, expire après `STATUS_TTL` = 5 s), `input_buffer`,
`watchlist_path`.

Les transitions qui ont besoin du réseau **retournent** des `AppCommand` au lieu
de les envoyer : `initial_commands`, `refresh_commands`, `tick`, `open_chart`,
`change_interval`, `request_add`. `App` reste testable sans thread ni réseau.

- Les résultats sont adressés **par symbole** : un résultat pour un ticker
  supprimé entre-temps est ignoré.
- `open_chart` recharge si les données de l'item ne sont pas dans
  `current_interval`.
- Refresh automatique toutes les `REFRESH_EVERY` (60 s), seulement si rien n'est
  en vol ; `r` force un refresh.
- Ajout et suppression réécrivent `watchlist.txt` ; une erreur d'écriture va dans
  la barre d'état.

### `ui/events.rs`

`handle_key(&mut App, KeyEvent, Instant) -> Vec<AppCommand>`, fonction pure :

1. Ctrl+C quitte partout.
2. En `InputMode`, **toutes** les touches vont à la saisie (lettres, chiffres,
   `- . = ^`, mises en majuscules) : `q` n'y déclenche jamais le quit.
3. Sinon, toute touche annule une confirmation en cours, puis le raccourci est
   interprété selon l'écran.

### `api/yahoo.rs`

- `http_client()` : client unique, timeout 10 s, User-Agent navigateur.
- `fetch_ticker_data(client, symbol, interval) -> FetchedTicker` : fenêtre
  `interval.history_days()`, `^` encodé en `%5E`. 404 → « SYMBOLE : symbole inconnu
  de Yahoo Finance » ; tous les messages commencent par le symbole.
- Parsing : chandelles avec un O/H/L/C `null` ignorées ; `result: null` ou aucune
  chandelle → erreur lisible.

### `models/`

| Interval     | Yahoo | Historique |
| ------------ | ----- | ---------- |
| M5           | `5m`  | 14 j       |
| M15          | `15m` | 30 j       |
| M30 (défaut) | `30m` | 58 j       |
| H1           | `1h`  | 180 j      |
| H4           | `4h`  | 365 j      |
| D1           | `1d`  | 730 j      |
| W1           | `1wk` | 3650 j     |

Yahoo limite l'intraday (< 1 h) à 60 jours. `OHLCData::utc_offset` vient de
`meta.gmtoffset` ; `local_time` et `previous_session_close` raisonnent en dates de
la place. La **variation du jour** (`Quote::change_percent`) compare
`regularMarketPrice` à la clôture de la séance précédente, et survit à un
chargement W1 (qui ne sait pas la calculer).

## Limites connues

- Heure de la place = décalage fixe au moment du chargement (voir
  [chart-rendering.md](./chart-rendering.md)).
- `logs/` est relatif au répertoire de lancement.
- Le worker traite les commandes une par une : une watchlist de plusieurs
  dizaines de tickers se rafraîchit en quelques secondes.
