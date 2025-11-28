# Architecture de LazyWallet

> Documentation d'architecture générale du projet LazyWallet
> Dernière mise à jour : 2025-01-28

## 📋 Table des Matières

- [Vue d'Ensemble](#vue-densemble)
- [Architecture des Modules](#architecture-des-modules)
- [Flux de Données](#flux-de-données)
- [Technologies Clés](#technologies-clés)
- [Patterns et Concepts](#patterns-et-concepts)
- [État de l'Application](#état-de-lapplication)
- [Cycle de Vie](#cycle-de-vie)
- [Extensions Futures](#extensions-futures)

---

## Vue d'Ensemble

### Qu'est-ce que LazyWallet ?

LazyWallet est une **application TUI (Terminal User Interface)** pour surveiller les marchés financiers en temps réel. Elle permet de :

- 📊 Afficher une watchlist de tickers (actions, indices, crypto)
- 📈 Visualiser des graphiques en chandeliers japonais (candlesticks)
- ⏱️ Changer d'intervalles de temps (1m, 5m, 1h, 1d, 1w, etc.)
- 🔄 Recharger les données automatiquement
- ⌨️ Navigation au clavier (style Vim)

### Caractéristiques Techniques

- **Langage** : Rust 2021 Edition
- **Runtime** : Synchrone (main) + Asynchrone (workers)
- **Interface** : TUI avec Ratatui + Crossterm
- **API** : Yahoo Finance (données OHLC)
- **Architecture** : Event-driven avec channels et state machine

---

## Architecture des Modules

```
lazywallet/
├── src/
│   ├── main.rs              # Point d'entrée, event loop
│   ├── lib.rs               # Exports publics
│   ├── app.rs               # État global de l'application
│   ├── api/                 # Couche API externe
│   │   ├── mod.rs
│   │   └── yahoo.rs         # Client Yahoo Finance
│   ├── models/              # Structures de données
│   │   ├── mod.rs
│   │   ├── ohlc.rs          # OHLC, Interval, Timeframe
│   │   ├── ticker.rs        # Ticker symbol
│   │   └── watchlist_item.rs # Item de la watchlist
│   └── ui/                  # Couche interface utilisateur
│       ├── mod.rs
│       ├── events.rs        # Gestion des événements clavier
│       ├── dashboard.rs     # Vue watchlist
│       ├── chart.rs         # Vue graphique (deprecated)
│       └── candlestick_text.rs # Rendu des chandeliers
├── docs/                    # Documentation
│   ├── architecture.md      # Ce fichier
│   └── candlestick-alignment.md
└── Cargo.toml              # Dépendances
```

### Module `main.rs` - Point d'Entrée

**Responsabilités** :
- Initialiser le terminal en mode raw
- Créer l'event loop principal
- Gérer les workers threads (async/sync)
- Coordonner le rendering et les événements

**Composants clés** :
```rust
enum AppCommand {
    ReloadTickerData { symbol, interval, index },
    AddTicker { symbol },
}

enum AppResult {
    TickerDataLoaded { index, data },
    TickerAdded { symbol, name, data },
    LoadError { index, error },
}
```

**Pattern** : Command pattern avec channels MPSC

### Module `app.rs` - État Global

**Responsabilités** :
- Centraliser tout l'état de l'application
- Fournir l'API pour modifier l'état
- Implémenter la state machine (Dashboard ↔ ChartView)

**Structure principale** :
```rust
pub struct App {
    pub running: bool,
    pub watchlist: Vec<WatchlistItem>,
    pub selected_index: usize,
    pub current_screen: Screen,
    pub current_interval: Interval,
    pub confirm_quit: bool,
    pub is_loading: bool,
    pub loading_message: Option<String>,
    pub input_buffer: String,
    pub input_prompt: String,
}

pub enum Screen {
    Dashboard,      // Vue watchlist
    ChartView,      // Vue graphique
    InputMode,      // Mode saisie
}
```

**Pattern** : State Management centralisé

### Module `api/` - Couche API

#### `api/yahoo.rs`

**Responsabilités** :
- Appeler l'API Yahoo Finance
- Parser les réponses JSON
- Convertir en structures Rust (`OHLCData`)

**Fonction principale** :
```rust
pub async fn fetch_ticker_data(
    symbol: &str,
    interval: Interval,
    timeframe: Timeframe
) -> Result<OHLCData>
```

**Flux** :
1. Construit l'URL avec paramètres (interval, range)
2. Envoie la requête HTTP GET
3. Parse la réponse JSON
4. Extrait timestamps, open, high, low, close
5. Retourne `OHLCData` avec vecteur de `OHLC`

**Pattern** : Repository pattern (abstraction de la source de données)

### Module `models/` - Structures de Données

#### `models/ohlc.rs`

**Structures clés** :
```rust
pub struct OHLC {
    pub timestamp: DateTime<Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
}

pub struct OHLCData {
    pub interval: Interval,
    pub timeframe: Timeframe,
    pub candles: Vec<OHLC>,
}

pub enum Interval {
    M5, M15, M30, H1, H4, D1, W1
}

pub enum Timeframe {
    OneDay, OneWeek, OneMonth,
    ThreeMonths, SixMonths,
    OneYear, TwoYears
}
```

**Responsabilités** :
- Représenter les données de marché
- Mapper intervalles ↔ timeframes
- Fournir formats d'affichage

**Pattern** : Domain Model

#### `models/watchlist_item.rs`

**Structure** :
```rust
pub struct WatchlistItem {
    pub symbol: String,
    pub name: String,
    pub data: Option<OHLCData>,
}
```

**Responsabilités** :
- Représenter un ticker dans la watchlist
- Calculer prix actuel et variation
- Formatter pour l'affichage

**Méthodes clés** :
- `current_price() -> Option<f64>`
- `change_percent() -> Option<f64>`
- `display() -> String`

### Module `ui/` - Interface Utilisateur

#### `ui/events.rs`

**Responsabilités** :
- Capturer les événements clavier
- Gérer le délai entre les touches
- Fournir une API non-bloquante

**Structure** :
```rust
pub struct EventHandler {
    rx: Receiver<Event>,
}

impl EventHandler {
    pub fn poll_event(&self) -> Option<Event>
}
```

**Pattern** : Observer pattern (écoute des événements)

#### `ui/dashboard.rs`

**Responsabilités** :
- Rendre la vue watchlist
- Afficher les tickers avec prix et variations
- Gérer les couleurs (vert/rouge)

**Fonction principale** :
```rust
pub fn render_dashboard(frame: &mut Frame, app: &App, area: Rect)
```

**Rendu** :
```
┌─ 💼 Watchlist ──────────────────────────────┐
│  AAPL     Apple Inc.            $175.43  ▲ +2.15% │
│  GOOGL    Alphabet Inc.          $142.56  ▼ -0.82% │
│  MSFT     Microsoft Corporat…   $378.91  ▲ +1.03% │
└─────────────────────────────────────────────┘
[a] Ajouter  [r] Refresh  [Enter] Chart  [q] Quit
```

**Pattern** : View component (MVC)

#### `ui/candlestick_text.rs`

**Responsabilités** :
- Rendre les graphiques en chandeliers japonais
- Gérer l'alignement chandeliers ↔ timestamps
- Adapter au redimensionnement du terminal

**Structures clés** :
```rust
pub struct CandlestickRenderer<'a> {
    candles: &'a [OHLC],
    interval: Interval,
    min_price: f64,
    max_price: f64,
    height: u16,
    width: u16,
    y_axis_width: u16,
}

struct CandlePosition {
    column: usize,
    width: usize,
}
```

**Algorithme** :
1. Sélectionne les chandeliers visibles (`visible_candles()`)
2. Calcule les positions exactes (`compute_candle_positions()`)
3. Rend ligne par ligne avec tableaux de caractères
4. Utilise les mêmes positions pour l'axe X

**Pattern** : Position Array + Accumulator Pattern

Voir [candlestick-alignment.md](./candlestick-alignment.md) pour les détails.

---

## Flux de Données

### 1. Démarrage de l'Application

```
┌──────────┐
│  main()  │
└────┬─────┘
     │
     ├─► Initialise le terminal (raw mode, alternate screen)
     ├─► Crée App avec watchlist initiale
     ├─► Lance worker thread (tokio runtime)
     ├─► Crée channels (command_tx, result_rx)
     └─► Entre dans l'event loop
```

### 2. Event Loop Principal

```
┌─────────────────────────────────────────┐
│          Event Loop (main.rs)           │
│                                         │
│  loop {                                 │
│    1. Poll événement clavier ───┐      │
│    2. Traiter événement         │      │
│    3. Poll résultats workers ───┼──┐   │
│    4. Mettre à jour App         │  │   │
│    5. Render UI                 │  │   │
│  }                              │  │   │
└─────────────────────────────────┼──┼───┘
                                  │  │
                    ┌─────────────┘  │
                    ▼                │
            ┌──────────────┐         │
            │ EventHandler │         │
            │  (events.rs) │         │
            └──────────────┘         │
                                     │
                    ┌────────────────┘
                    ▼
            ┌──────────────┐
            │ Worker Thread│
            │   (tokio)    │
            └──────────────┘
```

### 3. Ajout d'un Ticker

```
Utilisateur appuie sur 'a'
        │
        ▼
┌───────────────────┐
│ Screen::InputMode │ ◄── Mode saisie activé
└────────┬──────────┘
         │ Utilisateur tape "AAPL" + Enter
         ▼
┌───────────────────────┐
│ AppCommand::AddTicker │ ◄── Commande envoyée au worker
└────────┬──────────────┘
         │ Via channel (command_tx)
         ▼
┌────────────────────────┐
│ Worker Thread (async)  │
│                        │
│ 1. Fetch Yahoo Finance │
│ 2. Parse JSON          │
│ 3. Crée OHLCData       │
└────────┬───────────────┘
         │ Via channel (result_rx)
         ▼
┌─────────────────────────┐
│ AppResult::TickerAdded  │ ◄── Résultat reçu
└────────┬────────────────┘
         │ Event loop poll le résultat
         ▼
┌────────────────────────┐
│ app.watchlist.push()   │ ◄── Ticker ajouté à la watchlist
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ Render dashboard       │ ◄── UI mise à jour
└────────────────────────┘
```

### 4. Changement d'Intervalle

```
Utilisateur dans ChartView appuie sur 'h' (intervalle précédent)
        │
        ▼
┌──────────────────────────┐
│ app.current_interval = H1│ ◄── État mis à jour immédiatement
└────────┬─────────────────┘
         │
         ▼
┌────────────────────────────────┐
│ AppCommand::ReloadTickerData   │ ◄── Commande de reload
└────────┬───────────────────────┘
         │
         ▼
┌────────────────────────┐
│ Worker fetch nouvelles │
│ données avec H1        │
└────────┬───────────────┘
         │
         ▼
┌─────────────────────────────┐
│ AppResult::TickerDataLoaded │
└────────┬────────────────────┘
         │
         ▼
┌──────────────────────────────┐
│ watchlist[index].data = data │ ◄── Données mises à jour
└────────┬─────────────────────┘
         │
         ▼
┌────────────────────────┐
│ Render chart avec H1   │
└────────────────────────┘
```

### 5. Rendering

```
┌────────────────────┐
│ render(frame, app) │
└────────┬───────────┘
         │ Match sur app.current_screen
         ▼
    ┌────┴────┐
    │         │
    ▼         ▼
Dashboard  ChartView
    │         │
    │         ├─► render_header()
    │         ├─► render_candlestick_chart()
    │         │   │
    │         │   ├─► CandlestickRenderer::new()
    │         │   ├─► compute_candle_positions()
    │         │   ├─► render_lines()
    │         │   └─► render_x_axis()
    │         │
    ├─► render_dashboard()
    │   │
    │   ├─► Render watchlist items
    │   ├─► Couleurs (vert/rouge)
    │   └─► Shortcuts
    │
    └─► Affichage final dans le terminal
```

---

## Technologies Clés

### Runtime et Async

**Tokio** : Runtime asynchrone pour Rust
- Permet d'exécuter du code async/await
- Utilisé dans le worker thread
- Gère les I/O non-bloquantes (HTTP)

**Architecture hybride** :
- **Main thread** : Synchrone (event loop, rendering)
- **Worker thread** : Asynchrone (API calls)
- **Communication** : Channels MPSC

```rust
// Dans main.rs
let rt = tokio::runtime::Runtime::new()?;
thread::spawn(move || {
    rt.block_on(async {
        // Code async ici
    });
});
```

### Interface Utilisateur

**Ratatui** : Framework TUI moderne
- Widgets (List, Paragraph, Block, etc.)
- Layout system (Constraint, Direction)
- Styles et couleurs
- Backend-agnostic (Crossterm, Termion, etc.)

**Crossterm** : Backend pour le terminal
- Raw mode (capture toutes les touches)
- Alternate screen (garde le terminal propre)
- Event system (clavier, souris, resize)

**Pattern de rendu** :
```rust
terminal.draw(|frame| {
    render(frame, &app);  // Immutable borrow de app
})?;
```

### Gestion des Erreurs

**Anyhow** : Error handling ergonomique
- `Result<T, anyhow::Error>` pour toutes les fonctions faillibles
- `.context()` pour ajouter du contexte aux erreurs
- `?` operator pour propager les erreurs

```rust
pub async fn fetch_ticker_data(...) -> Result<OHLCData> {
    let response = client.get(url)
        .send().await
        .context("Failed to send request")?;  // ← Context ajouté
    // ...
}
```

### Logging

**Tracing** : Logging structuré
- Levels : trace, debug, info, warn, error
- Spans pour contexte hierarchique
- Compatible avec tokio (async-aware)

**Configuration** :
- Variable d'environnement `RUST_LOG` pour filtrer
- Logs dans `~/.local/share/lazywallet/logs/`
- Rotation quotidienne automatique

```rust
tracing::info!(symbol = %ticker.symbol, "Fetching data");
```

### Sérialisation

**Serde** : Framework de sérialisation/désérialisation
- `#[derive(Serialize, Deserialize)]` sur les structs
- Support JSON, YAML, TOML, etc.
- Utilisé pour parser les réponses Yahoo Finance

```rust
#[derive(Debug, Deserialize)]
struct ChartResult {
    timestamp: Vec<i64>,
    indicators: Indicators,
    meta: Meta,
}
```

---

## Patterns et Concepts

### 1. State Machine (Écrans)

**Pattern** : Finite State Machine (FSM)

```rust
pub enum Screen {
    Dashboard,   // État 1
    ChartView,   // État 2
    InputMode,   // État 3
}
```

**Transitions** :
- `Dashboard → ChartView` : Touche Enter
- `ChartView → Dashboard` : Touche ESC
- `* → InputMode` : Touche 'a' (add)
- `InputMode → *` : Touche Enter (valider) ou ESC (annuler)

**Avantage** : Un seul écran actif, pas d'état incohérent.

### 2. Command Pattern (Workers)

**Pattern** : Command + Observer

```rust
// Command
enum AppCommand {
    ReloadTickerData { ... },
    AddTicker { ... },
}

// Sender envoie des commandes
command_tx.send(AppCommand::AddTicker { symbol })?;

// Receiver exécute les commandes
while let Ok(cmd) = command_rx.recv() {
    match cmd {
        AppCommand::AddTicker { symbol } => {
            // Exécute async
        }
    }
}
```

**Avantage** : Découplage, main thread non bloqué.

### 3. Repository Pattern (API)

**Pattern** : Repository

```rust
pub async fn fetch_ticker_data(
    symbol: &str,
    interval: Interval,
    timeframe: Timeframe
) -> Result<OHLCData>
```

**Abstraction** :
- La couche UI ne connaît pas Yahoo Finance
- On pourrait changer pour Alpha Vantage, IEX, etc.
- Seul `api/yahoo.rs` change

**Avantage** : Changement de source de données facile.

### 4. Two-Step Quit

**Pattern** : Confirmation de sortie

```rust
if key_code == KeyCode::Char('q') {
    if app.confirm_quit {
        app.running = false;  // Vraie sortie
    } else {
        app.confirm_quit = true;  // Demande confirmation
    }
}
```

**Avantage** : Évite les sorties accidentelles.

### 5. RAII (Terminal Cleanup)

**Pattern** : Resource Acquisition Is Initialization

```rust
fn setup_terminal() -> Result<Terminal<...>> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    // Terminal créé
}

fn restore_terminal(...) -> Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    // Terminal restauré
}

// En cas de panic ou erreur, restore_terminal() est appelé
```

**Avantage** : Terminal toujours restauré, même en cas de panic.

### 6. Position Array (Rendering)

**Pattern** : Single Source of Truth + Accumulator

Voir [candlestick-alignment.md](./candlestick-alignment.md)

**Principe** :
- Une seule fonction calcule toutes les positions
- Toutes les couches (chandeliers, labels) utilisent les mêmes positions
- Garantit l'alignement parfait

---

## État de l'Application

### Structure `App`

```rust
pub struct App {
    // Lifecycle
    pub running: bool,           // Continue ou quitte ?
    pub confirm_quit: bool,      // Attend confirmation ?

    // Data
    pub watchlist: Vec<WatchlistItem>,  // Liste des tickers
    pub selected_index: usize,          // Index sélectionné

    // UI State
    pub current_screen: Screen,         // Écran actif
    pub current_interval: Interval,     // Intervalle graphique

    // Loading State
    pub is_loading: bool,               // En chargement ?
    pub loading_message: Option<String>, // Message de chargement

    // Input State
    pub input_buffer: String,           // Buffer de saisie
    pub input_prompt: String,           // Prompt affiché
}
```

### Méthodes Principales

```rust
impl App {
    pub fn new() -> Self                     // Constructeur
    pub fn next_ticker(&mut self)            // Sélection suivante
    pub fn previous_ticker(&mut self)        // Sélection précédente
    pub fn next_interval(&mut self)          // Intervalle suivant
    pub fn previous_interval(&mut self)      // Intervalle précédent
    pub fn quit(&mut self)                   // Quitter l'app
    pub fn is_loading_data(&self) -> bool    // Check si en chargement
    pub fn is_awaiting_quit_confirmation(&self) -> bool
}
```

### Invariants

**Invariants maintenus** :
1. `selected_index < watchlist.len()` (sauf watchlist vide)
2. Un seul `Screen` actif à la fois
3. `confirm_quit = true` implique affichage de confirmation
4. `is_loading = true` implique affichage d'indicateur

**Responsabilité** : Toutes les méthodes de `App` préservent ces invariants.

---

## Cycle de Vie

### 1. Initialisation

```rust
fn main() -> Result<()> {
    // 1. Setup logging
    setup_logging()?;

    // 2. Setup terminal
    let mut terminal = setup_terminal()?;

    // 3. Create app state
    let mut app = App::new();

    // 4. Load initial tickers
    app.watchlist = vec![
        WatchlistItem::new("AAPL".to_string()),
        WatchlistItem::new("GOOGL".to_string()),
        // ...
    ];

    // 5. Start worker thread
    let (command_tx, command_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    thread::spawn(move || worker_thread(command_rx, result_tx));

    // 6. Enter event loop
    run(&mut terminal, &mut app, command_tx, result_rx)?;

    // 7. Cleanup
    restore_terminal(&mut terminal)?;
}
```

### 2. Event Loop

```rust
fn run(...) -> Result<()> {
    let event_handler = EventHandler::new();

    while app.running {
        // 1. Render UI
        terminal.draw(|frame| render(frame, &app))?;

        // 2. Poll keyboard events
        if let Some(event) = event_handler.poll_event()? {
            handle_event(&mut app, event, &command_tx)?;
        }

        // 3. Poll worker results
        if let Ok(result) = result_rx.try_recv() {
            handle_result(&mut app, result);
        }

        // 4. Small sleep to avoid busy-wait
        thread::sleep(Duration::from_millis(10));
    }
}
```

### 3. Gestion des Événements

```rust
fn handle_event(app: &mut App, event: Event, command_tx: &Sender<AppCommand>) -> Result<()> {
    match event {
        Event::Key(key_event) => match app.current_screen {
            Screen::Dashboard => handle_dashboard_input(app, key_event, command_tx)?,
            Screen::ChartView => handle_chart_input(app, key_event, command_tx)?,
            Screen::InputMode => handle_input_mode(app, key_event, command_tx)?,
        },
        Event::Resize(_, _) => {
            // Terminal redimensionné, le prochain render s'adaptera
        }
        _ => {}
    }
    Ok(())
}
```

### 4. Worker Thread

```rust
fn worker_thread(command_rx: Receiver<AppCommand>, result_tx: Sender<AppResult>) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        while let Ok(command) = command_rx.recv() {
            match command {
                AppCommand::ReloadTickerData { symbol, interval, index } => {
                    match fetch_ticker_data(&symbol, interval, ...).await {
                        Ok(data) => {
                            result_tx.send(AppResult::TickerDataLoaded { index, data }).ok();
                        }
                        Err(err) => {
                            result_tx.send(AppResult::LoadError { index, error }).ok();
                        }
                    }
                }
                AppCommand::AddTicker { symbol } => {
                    // Similar async fetch
                }
            }
        }
    });
}
```

### 5. Cleanup

```rust
fn restore_terminal(terminal: &mut Terminal<...>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}
```

**Appelé** :
- À la sortie normale (`app.running = false`)
- En cas d'erreur (via `?`)
- En cas de panic (via hooks)

---

## Extensions Futures

### Fonctionnalités Prévues

#### 1. Sauvegarde de la Watchlist

**Objectif** : Persister la watchlist entre les sessions

**Implémentation** :
```rust
// Dans models/watchlist_item.rs
impl WatchlistItem {
    pub fn save_to_file(watchlist: &[WatchlistItem], path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(watchlist)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn load_from_file(path: &Path) -> Result<Vec<WatchlistItem>> {
        let json = fs::read_to_string(path)?;
        let watchlist = serde_json::from_str(&json)?;
        Ok(watchlist)
    }
}
```

**Fichier** : `~/.config/lazywallet/watchlist.json`

#### 2. Indicateurs Techniques

**Objectif** : Overlay MA, RSI, Bollinger sur les graphiques

**Implémentation** :
```rust
// Nouveau module indicators/
pub fn moving_average(data: &[f64], period: usize) -> Vec<f64>
pub fn rsi(data: &[OHLC], period: usize) -> Vec<f64>
pub fn bollinger_bands(data: &[f64], period: usize, std_dev: f64) -> (Vec<f64>, Vec<f64>)
```

**Rendu** : Utiliser les mêmes positions que les chandeliers.

#### 3. Multi-Timeframe View

**Objectif** : Afficher plusieurs intervalles côte à côte

**Layout** :
```
┌─────────────┬─────────────┬─────────────┐
│    M15      │     H1      │     D1      │
│  [Chart]    │  [Chart]    │  [Chart]    │
└─────────────┴─────────────┴─────────────┘
```

**État** :
```rust
pub struct App {
    pub multi_view_intervals: Vec<Interval>,  // [M15, H1, D1]
}
```

#### 4. Alertes de Prix

**Objectif** : Notifier quand un prix franchit un seuil

**Implémentation** :
```rust
pub struct PriceAlert {
    symbol: String,
    condition: AlertCondition,  // Above, Below
    threshold: f64,
    triggered: bool,
}
```

**Worker** : Vérifier les alertes à chaque fetch.

#### 5. Export de Données

**Objectif** : Exporter les données en CSV

**Format** :
```csv
timestamp,open,high,low,close
2025-01-28 09:30:00,175.00,175.50,174.80,175.20
```

#### 6. Zoom et Pan

**Objectif** : Naviguer dans l'historique

**Controls** :
- `←` / `→` : Pan horizontal
- `+` / `-` : Zoom in/out

**État** :
```rust
pub struct ChartState {
    start_index: usize,    // Début de la fenêtre
    visible_count: usize,  // Nombre de chandeliers visibles
}
```

---

## Maintenance

### Guidelines de Code

**Style Rust** :
- Suivre `rustfmt` (format automatique)
- Suivre `clippy` (lints)
- Documenter avec `///` (doc comments)
- Tests unitaires pour logique complexe

**Naming Conventions** :
- Structures : `PascalCase`
- Fonctions : `snake_case`
- Constantes : `SCREAMING_SNAKE_CASE`
- Modules : `snake_case`

**Error Handling** :
- Utiliser `Result<T>` partout
- Ajouter context avec `.context()`
- Log les erreurs avec `tracing::error!`

**Documentation** :
- Mettre à jour `docs/architecture.md` lors de changements structurels
- Documenter les décisions techniques complexes
- Ajouter des exemples dans les doc comments

### Commandes Utiles

```bash
# Build
cargo build
cargo build --release

# Run
cargo run
RUST_LOG=debug cargo run  # Avec logs debug

# Test
cargo test
cargo test --lib  # Tests unitaires seulement

# Lint
cargo fmt  # Format code
cargo clippy  # Lint
cargo clippy -- -D warnings  # Lint strict

# Documentation
cargo doc --open  # Générer et ouvrir la doc
```

### Fichiers de Configuration

**Logs** : `~/.local/share/lazywallet/logs/`
- `app.log.YYYY-MM-DD` : Logs quotidiens
- Rotation automatique

**Future watchlist** : `~/.config/lazywallet/watchlist.json`

---

## Références

### Documentation Externe

- [Ratatui Book](https://ratatui.rs/)
- [Crossterm Docs](https://docs.rs/crossterm/)
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [Rust Book](https://doc.rust-lang.org/book/)

### Documentation Interne

- [candlestick-alignment.md](./candlestick-alignment.md) - Stratégie d'alignement des chandeliers
- [Cargo.toml](../Cargo.toml) - Dépendances commentées

---

*Documentation maintenue par : @cyril*
*Dernière mise à jour : 2025-01-28*
*Version : 0.1.0*
