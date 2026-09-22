# Architecture de LazyWallet

> Décrit le code tel qu'il est sur `main` (revue du 2026-09-22).
> Les évolutions envisagées sont dans la Roadmap du README, pas ici.

## Vue d'ensemble

TUI Rust (ratatui 0.26 + crossterm 0.27) qui affiche une watchlist et un graphique en
chandeliers pour le ticker sélectionné. Source unique de données : l'endpoint
`v8/finance/chart` de Yahoo Finance, sans clé ni cookie.

Deux threads :

- **Thread principal** (synchrone) : event loop, rendu, gestion clavier.
- **Worker** (runtime tokio dédié, `block_on` par commande) : appels HTTP.

Ils communiquent par deux canaux `std::sync::mpsc` et partagent `Arc<Mutex<App>>`.

## Modules

```
src/
├── main.rs              # logging, chargement initial, worker, event loop, handle_event
├── lib.rs               # expose api, models, app, ui (pour les tests)
├── app.rs               # App (état global) + Screen
├── api/yahoo.rs         # fetch_ticker_data, build_yahoo_url, parse_yahoo_response
├── models/
│   ├── ohlc.rs          # OHLC, OHLCData, Interval, Timeframe, LabelStrategy, AxisFormats
│   ├── watchlist_item.rs# WatchlistItem (prix courant, variation)
│   └── ticker.rs        # TickerType::from_symbol (Ticker lui-même est inutilisé)
└── ui/
    ├── mod.rs           # render = dashboard::render
    ├── events.rs        # EventHandler (poll 250 ms) + prédicats is_*_event
    ├── dashboard.rs     # routage par Screen, watchlist, footer, saisie
    ├── candlestick_text.rs # CandlestickRenderer + écran graphique
    └── chart.rs         # ancien graphique ligne, plus appelé
```

### `main.rs`

- `init_logging()` : `tracing` vers `./logs/lazywallet.log.YYYY-MM-DD` (relatif au
  répertoire courant, rotation quotidienne). Filtre par défaut `lazywallet=debug,info`,
  surchargé par `RUST_LOG`.
- `load_watchlist_data()` : charge en séquence `AAPL`, `TSLA`, `BTC-USD` (intervalle
  30m, 500 ms entre requêtes) **avant** d'ouvrir le TUI. Un échec donne un item sans
  données (« Loading... » permanent).
- `spawn_background_worker()` : consomme `AppCommand`, publie `AppResult`, et
  bascule `App::is_loading` autour de chaque fetch.
- `run()` : à chaque tour — un `try_recv` de résultat, un rendu, un événement
  (bloquant jusqu'à 250 ms), puis `App::tick()` (vide).
- `handle_event()` : un seul `match` à gardes ; **l'ordre des bras compte** (voir
  Limites).

```rust
enum AppCommand { ReloadTickerData { symbol, interval, index }, AddTicker { symbol } }
enum AppResult  { TickerDataLoaded { index, data }, TickerAdded { symbol, name, data },
                  LoadError { index, symbol, error }, AddError { symbol, error } }
```

### `app.rs`

`App` porte tout l'état : `watchlist`, `selected_index`, `current_screen`
(`Dashboard | ChartView | InputMode`), `current_interval` (global, pas par ticker),
les drapeaux `confirm_quit` / `confirm_delete` (confirmation en deux appuis),
`is_loading` + `loading_message`, `input_buffer` + `input_prompt`.
Tous les champs sont `pub` ; les méthodes sont des setters sans invariants forts.

### `api/yahoo.rs`

`fetch_ticker_data(symbol, interval) -> Result<(OHLCData, Option<String>)>` :

1. `period1/period2` = maintenant − `interval.default_timeframe().to_days()`.
2. Nouveau `reqwest::Client` à chaque appel, User-Agent navigateur, **sans timeout**.
3. HTTP non-2xx → erreur ; sinon désérialisation serde.
4. Les chandelles avec un O/H/L/C `null` sont ignorées ; aucune chandelle → erreur.
5. Retourne aussi `meta.longName` (nom affiché).

### `models/ohlc.rs`

| Interval     | Yahoo | Timeframe demandé | Axe X                                  |
| ------------ | ----- | ----------------- | -------------------------------------- |
| M5           | `5m`  | 7 j               | heures rondes /1h + changement de jour |
| M15          | `15m` | 14 j              | /3h + jour                             |
| M30 (défaut) | `30m` | 30 j              | /6h + jour                             |
| H1           | `1h`  | 6 mois            | `RegularDays{2}`                       |
| H4           | `4h`  | 1 an              | mois                                   |
| D1           | `1d`  | 2 ans             | mois                                   |
| W1           | `1wk` | 5 ans             | années                                 |

`OHLCData::daily_change_percent()` : pour D1/W1, variation de la dernière chandelle ;
en intraday, `close` de la dernière chandelle vs `open` de la première chandelle du
même jour **UTC**. `TickerType` est déduit du symbole (`-USD` → Crypto, `^` → Index,
`=` → Forex, petite liste d'ETF, sinon Stock) ; il est passé à `x_axis_format` mais
pas encore utilisé.

### `ui/candlestick_text.rs`

`CandlestickRenderer` produit des `Line` ratatui : axe Y (prix toutes les 4 lignes),
une ligne de caractères par rangée, puis 3 lignes d'axe X (ticks, heures, dates).
Les 250 dernières chandelles sont gardées, leurs colonnes calculées une fois par
`compute_candle_positions()` et réutilisées par toutes les couches.
Détails et limites : [candlestick-alignment.md](./candlestick-alignment.md).

## Flux

**Ajout** : `a` → `InputMode` → Entrée → `AddTicker` au worker → fetch 30m →
`TickerAdded` → `watchlist.push`. En cas d'erreur : `AddError`, seulement loggé.

**Changement d'intervalle** (graphique, `h`/`l`) : `current_interval` change tout de
suite → `ReloadTickerData { index }` → le worker fetch → `TickerDataLoaded` remplace
`watchlist[index].data`. Tant que la donnée n'est pas arrivée, le titre affiche
`30m → 1h ⚠️`.

**Rendu** : `ui::render` route sur `current_screen` ; `InputMode` réaffiche le
dashboard avec la ligne de saisie en footer.

## Limites connues de l'architecture

- `handle_event` teste `q` avant l'écran courant : `q` en saisie déclenche le quit.
- Les résultats sont adressés par **index** : supprimer un ticker pendant un reload
  écrit la donnée sur le mauvais item.
- `current_interval` est global alors que chaque item garde les données du dernier
  intervalle chargé pour lui ; la variation du dashboard en dépend.
- Pas de panic hook : un panic laisse le terminal en raw mode. Les `Mutex::lock().unwrap()`
  propagent un empoisonnement au thread suivant.
- Erreurs de lecture d'événement ignorées dans `run()`, `Event::Error` jamais émis.
- Code mort : `ui/chart.rs`, `Ticker`, `WatchlistItem::display`, `Interval::all`,
  `OHLCData::with_interval`, les dérivés serde des modèles, la dépendance `dirs`.
