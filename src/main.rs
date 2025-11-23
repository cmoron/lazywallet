// ============================================================================
// LazyWallet - Phase 2 Étape 2 : Watchlist interactive
// ============================================================================
// Programme TUI avec watchlist de tickers et navigation
// Charge les prix depuis Yahoo Finance et affiche avec couleurs
//
// CONCEPTS RUST CLÉS :
// 1. Terminal raw mode : contrôle total du terminal
// 2. Event loop : boucle infinie qui gère événements et rendering
// 3. Async dans sync : tokio::runtime::Runtime pour appels API
// 4. RAII : restauration automatique du terminal avec Drop
// ============================================================================

use std::io;
use std::sync::{Arc, Mutex, mpsc};

use anyhow::{Context, Result};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tracing::{debug, error, info};

use lazywallet::api::yahoo::fetch_ticker_data;
use lazywallet::app::App;
use lazywallet::models::{Interval, OHLCData, WatchlistItem};
use lazywallet::ui::{events::EventHandler, render};

// ============================================================================
// AppCommand : Commandes pour le worker thread
// ============================================================================
// CONCEPT RUST : Command pattern avec channels
// - L'event loop envoie des commandes au worker thread
// - Le worker thread exécute les tâches async (fetch API)
// - Communication via mpsc channels (multi-producer, single-consumer)
// ============================================================================

/// Commandes envoyées au worker thread pour exécuter des tâches async
#[derive(Debug, Clone)]
enum AppCommand {
    /// Recharger les données d'un ticker avec un nouvel intervalle
    /// CONCEPT : Background data loading
    /// - symbol: ticker à recharger (ex: "AAPL")
    /// - interval: nouvel intervalle (ex: Interval::M15)
    /// - index: position dans la watchlist
    ReloadTickerData {
        symbol: String,
        interval: Interval,
        index: usize,
    },

    /// Ajouter un nouveau ticker à la watchlist
    /// CONCEPT : Add ticker with background fetch
    /// - symbol: ticker à ajouter (ex: "GOOGL")
    /// - Les données seront fetchées automatiquement
    AddTicker {
        symbol: String,
    },
}

/// Résultats renvoyés par le worker thread
#[derive(Debug)]
enum AppResult {
    /// Données d'un ticker rechargées avec succès
    TickerDataLoaded {
        index: usize,
        data: OHLCData,
    },

    /// Nouveau ticker ajouté avec succès
    TickerAdded {
        symbol: String,
        name: String,
        data: OHLCData,
    },

    /// Erreur lors du chargement
    LoadError {
        index: usize,
        symbol: String,
        error: String,
    },

    /// Erreur lors de l'ajout d'un ticker
    AddError {
        symbol: String,
        error: String,
    },
}

// ============================================================================
// Initialisation du logging
// ============================================================================
// CONCEPT : Logging dans une app TUI
// - Les println! ne fonctionnent pas une fois le TUI lancé
// - On log vers un fichier à la place
// - Tracing : framework moderne de logging structuré
// - Rotation quotidienne automatique des logs
// ============================================================================

/// Initialise le système de logging vers fichier
///
/// CONCEPT RUST : Tracing subscriber
/// - Registry : point central des logs
/// - Layer : transforme et route les logs
/// - EnvFilter : filtre par niveau (RUST_LOG env var)
/// - RollingFileAppender : rotation automatique
///
/// Les logs sont écrits dans :
/// - Linux/WSL : ~/.local/share/lazywallet/logs/lazywallet.log
/// - macOS : ~/Library/Application Support/lazywallet/logs/lazywallet.log
/// - Windows : C:\Users\<user>\AppData\Local\lazywallet\logs\lazywallet.log
///
/// # Utilisation
/// ```bash
/// # Voir les logs en temps réel
/// tail -f ~/.local/share/lazywallet/logs/lazywallet.log
///
/// # Contrôler le niveau de log
/// RUST_LOG=debug cargo run
/// RUST_LOG=lazywallet=trace cargo run
/// ```
fn init_logging() -> Result<()> {
    use tracing_appender::rolling::{RollingFileAppender, Rotation};
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    let log_dir = std::path::PathBuf::from("./logs");

    // Crée le répertoire s'il n'existe pas
    std::fs::create_dir_all(&log_dir).context("Échec de la création du répertoire de logs")?;

    // Configure la rotation quotidienne des logs
    // CONCEPT : Log rotation
    // - Rotation::DAILY : nouveau fichier chaque jour
    // - Ancien format : lazywallet.log.2024-01-15
    // - Évite que les logs deviennent trop gros
    let file_appender = RollingFileAppender::new(Rotation::DAILY, log_dir.clone(), "lazywallet.log");

    // Configure le subscriber (receveur de logs)
    // CONCEPT : Builder pattern avec layers
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(file_appender) // Écrit dans le fichier
                .with_ansi(false) // Pas de codes couleur dans le fichier
                .with_target(true) // Inclut le module (ex: lazywallet::api::yahoo)
                .with_thread_ids(true) // Inclut l'ID du thread (utile pour async)
                .with_line_number(true) // Inclut le numéro de ligne
        )
        .with(
            // Filtre les logs par niveau
            // CONCEPT : EnvFilter
            // - RUST_LOG=debug : tous les logs debug+
            // - RUST_LOG=lazywallet=trace : trace pour lazywallet, info pour le reste
            // - Par défaut : debug pour lazywallet, info pour les dépendances
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "lazywallet=debug,info".into()),
        )
        .init();

    // Premier log : confirme que le logging est initialisé
    info!(?log_dir, "Logging initialisé");
    Ok(())
}

// ============================================================================
// Point d'entrée du programme
// ============================================================================
// CONCEPT RUST : Async dans sync
// - main() est synchrone (pour TUI)
// - Mais on a besoin d'async pour les appels API
// - Solution : tokio::runtime::Runtime pour exécuter du code async
// ============================================================================

fn main() -> Result<()> {
    // CONCEPT RUST : Exécuter du code async dans du code sync
    // - tokio::runtime::Runtime : crée un runtime tokio
    // - .block_on() : exécute une future de manière bloquante
    // - Permet de combiner async (API) et sync (TUI)

    // Initialize logging FIRST
    // CONCEPT : Logging avant tout le reste
    // - Si init échoue, on affiche l'erreur et continue quand même
    // - Permet d'avoir des logs pour tout le reste du programme
    init_logging().unwrap_or_else(|e| {
        eprintln!("⚠️  Warning: Failed to initialize logging: {}", e);
        eprintln!("   Continuing without logging...");
    });

    println!("LazyWallet starting up");
    info!("LazyWallet starting up");

    // Charge les données de la watchlist (appels API async)
    info!("Loading watchlist data");
    println!("📊 Chargement des données...\n");

    let runtime = tokio::runtime::Runtime::new()?;
    let watchlist = runtime.block_on(load_watchlist_data())?;

    info!("Watchlist data loaded successfully");
    println!("✅ Données chargées !\n");

    // Setup du terminal en mode TUI
    debug!("Setting up terminal");
    let mut terminal = setup_terminal()?;

    // Crée l'état de l'application avec les données chargées
    // CONCEPT RUST : Arc<Mutex<>> pour partage entre threads
    // - Arc : Reference counting pour ownership partagé
    // - Mutex : Protection contre les data races
    // - Permet au worker thread et à l'UI d'accéder à App
    let app = Arc::new(Mutex::new(App::with_watchlist(watchlist)));

    // Crée les channels pour communication avec le worker
    // CONCEPT RUST : mpsc channels
    // - (sender, receiver) : canal unidirectionnel
    // - command_tx/rx : pour envoyer des commandes au worker
    // - result_tx/rx : pour recevoir les résultats du worker
    let (command_tx, command_rx) = mpsc::channel::<AppCommand>();
    let (result_tx, result_rx) = mpsc::channel::<AppResult>();

    // Lance le worker thread en arrière-plan
    info!("Spawning background worker thread");
    spawn_background_worker(command_rx, result_tx, app.clone());

    // Crée le gestionnaire d'événements
    let events = EventHandler::new();

    // Exécute l'event loop
    info!("Starting event loop");
    let result = run(&mut terminal, app.clone(), &events, command_tx, result_rx);

    // Restaure le terminal (même en cas d'erreur)
    debug!("Restoring terminal");
    restore_terminal(&mut terminal)?;

    match &result {
        Ok(_) => info!("Application exited normally"),
        Err(e) => error!(error = ?e, "Application exited with error"),
    }

    // Retourne le résultat de run()
    result
}

// ============================================================================
// Chargement des données
// ============================================================================
// CONCEPT RUST : async fn
// - Fonction asynchrone qui peut faire des appels API
// - Retourne une Future<Output = Result<Vec<WatchlistItem>>>
// ============================================================================

/// Charge les données de la watchlist depuis Yahoo Finance
///
/// CONCEPT RUST : Async/await et gestion d'erreurs
/// - async fn : fonction qui retourne une Future
/// - .await : suspend jusqu'à résolution
/// - ? : propage les erreurs
async fn load_watchlist_data() -> Result<Vec<WatchlistItem>> {
    // Définit les tickers à charger
    // CONCEPT RUST : Array de tuples
    // - (symbol, name) pour chaque ticker
    let tickers = [
        ("AAPL", "Apple Inc."),
        ("TSLA", "Tesla"),
        ("BTC-USD", "Bitcoin USD"),
    ];

    let mut watchlist = Vec::new();

    // Charge chaque ticker
    // CONCEPT RUST : Loop avec enumerate
    for (i, &(symbol, name)) in tickers.iter().enumerate() {
        debug!(ticker = %symbol, progress = i + 1, total = tickers.len(), "Fetching ticker data");
        println!("  [{}/{}] Chargement de {}...", i + 1, tickers.len(), symbol);

        // Appel API pour récupérer les données
        // Utilise l'intervalle par défaut (30m)
        // Le timeframe est déterminé automatiquement par l'intervalle
        match fetch_ticker_data(symbol, Interval::default()).await {
            Ok(data) => {
                // Succès : crée un WatchlistItem avec les données
                info!(ticker = %symbol, candles = data.len(), "Ticker data fetched successfully");
                watchlist.push(WatchlistItem::with_data(
                    symbol.to_string(),
                    name.to_string(),
                    data,
                ));
                println!("    ✓ OK");
            }
            Err(e) => {
                // Erreur : affiche et crée un item sans données
                error!(ticker = %symbol, error = ?e, "Failed to fetch ticker data");
                watchlist.push(WatchlistItem::new(
                    symbol.to_string(),
                    name.to_string(),
                ));
            }
        }

        // Petit délai entre les requêtes (rate limiting)
        if i < tickers.len() - 1 {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    }

    Ok(watchlist)
}

// ============================================================================
// Background Worker Thread
// ============================================================================
// CONCEPT RUST : Background async worker avec channels
// - Thread séparé qui traite les commandes async
// - Reçoit des AppCommand via un channel (command_rx)
// - Envoie des AppResult via un autre channel (result_tx)
// - Permet de faire des appels API sans bloquer l'UI
// ============================================================================

/// Worker thread qui exécute les tâches async en arrière-plan
///
/// CONCEPT RUST : Thread + async runtime
/// - std::thread::spawn() : crée un thread OS
/// - tokio::runtime::Runtime : runtime async dans ce thread
/// - mpsc channels : communication inter-thread
///
/// # Arguments
/// * `command_rx` - Receiver pour recevoir les commandes
/// * `result_tx` - Sender pour envoyer les résultats
/// * `app` - Arc<Mutex<App>> pour accéder à l'état partagé
fn spawn_background_worker(
    command_rx: mpsc::Receiver<AppCommand>,
    result_tx: mpsc::Sender<AppResult>,
    app: Arc<Mutex<App>>,
) {
    std::thread::spawn(move || {
        // Crée un runtime tokio pour ce thread
        // CONCEPT : Runtime per-thread
        // - Chaque thread peut avoir son propre runtime
        // - Permet d'exécuter du code async dans un thread standard
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

        // Boucle de traitement des commandes
        // CONCEPT : Command processing loop
        // - Attend une commande sur command_rx
        // - Traite la commande de manière async
        // - Envoie le résultat sur result_tx
        loop {
            match command_rx.recv() {
                Ok(command) => {
                    info!(?command, "Worker received command");

                    match command {
                        AppCommand::ReloadTickerData { symbol, interval, index } => {
                            // Active l'indicateur de chargement
                            {
                                let mut app_lock = app.lock().unwrap();
                                app_lock.start_loading(Some(format!(
                                    "Chargement {} avec intervalle {}...",
                                    symbol,
                                    interval.label()
                                )));
                            }

                            // Exécute le fetch de manière async
                            // CONCEPT : block_on dans un worker thread
                            // - block_on() bloque le thread worker (pas l'UI)
                            // - L'UI continue à tourner normalement
                            let result = runtime.block_on(async {
                                fetch_ticker_data(&symbol, interval).await
                            });

                            match result {
                                Ok(data) => {
                                    info!(ticker = %symbol, interval = %interval.label(), candles = data.len(), "Data loaded successfully");
                                    let _ = result_tx.send(AppResult::TickerDataLoaded { index, data });
                                }
                                Err(e) => {
                                    error!(ticker = %symbol, error = ?e, "Failed to load ticker data");
                                    let _ = result_tx.send(AppResult::LoadError {
                                        index,
                                        symbol: symbol.clone(),
                                        error: e.to_string(),
                                    });
                                }
                            }

                            // Désactive l'indicateur de chargement
                            {
                                let mut app_lock = app.lock().unwrap();
                                app_lock.stop_loading();
                            }
                        }

                        AppCommand::AddTicker { symbol } => {
                            // Active l'indicateur de chargement
                            {
                                let mut app_lock = app.lock().unwrap();
                                app_lock.start_loading(Some(format!(
                                    "Ajout de {}...",
                                    symbol
                                )));
                            }

                            // Fetch les données avec l'intervalle par défaut
                            let result = runtime.block_on(async {
                                fetch_ticker_data(&symbol, Interval::default()).await
                            });

                            match result {
                                Ok(data) => {
                                    info!(ticker = %symbol, candles = data.len(), "Ticker added successfully");
                                    // Pour le nom, on utilise le symbol pour l'instant
                                    // TODO: Récupérer le nom réel depuis Yahoo Finance
                                    let _ = result_tx.send(AppResult::TickerAdded {
                                        symbol: symbol.clone(),
                                        name: symbol.clone(),
                                        data,
                                    });
                                }
                                Err(e) => {
                                    error!(ticker = %symbol, error = ?e, "Failed to add ticker");
                                    let _ = result_tx.send(AppResult::AddError {
                                        symbol: symbol.clone(),
                                        error: e.to_string(),
                                    });
                                }
                            }

                            // Désactive l'indicateur de chargement
                            {
                                let mut app_lock = app.lock().unwrap();
                                app_lock.stop_loading();
                            }
                        }
                    }
                }
                Err(_) => {
                    // Channel fermé, on quitte
                    info!("Worker thread exiting (channel closed)");
                    break;
                }
            }
        }
    });
}

// ============================================================================
// Event Loop Principal
// ============================================================================
// CONCEPT : Game Loop / Event Loop Pattern
// - Loop infinie : while app.is_running()
// - À chaque itération :
//   1. Traiter les événements (input)
//   2. Mettre à jour l'état (update)
//   3. Dessiner l'interface (render)
//
// C'est le pattern classique des jeux vidéo et applications interactives !
// ============================================================================

/// Exécute la boucle principale de l'application
///
/// CONCEPT RUST : Arc<Mutex<>> pour partage entre threads
/// - Arc<Mutex<App>> : app partagée entre UI et worker
/// - Mutex::lock() : obtenir accès exclusif temporaire
/// - command_tx : envoyer commandes au worker
/// - result_rx : recevoir résultats du worker
fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: Arc<Mutex<App>>,
    events: &EventHandler,
    command_tx: mpsc::Sender<AppCommand>,
    result_rx: mpsc::Receiver<AppResult>,
) -> Result<()> {
    // Loop infinie jusqu'à ce que app.running devienne false
    loop {
        // Vérifie si l'app est toujours en cours d'exécution
        // CONCEPT : Lock scope minimisé
        // - Lock seulement pour lire is_running
        // - Unlock immédiat après le if
        {
            let app_lock = app.lock().unwrap();
            if !app_lock.is_running() {
                break;
            }
        }

        // ========================================
        // 0. RÉSULTATS : Traite les résultats du worker
        // ========================================
        // CONCEPT : Non-blocking receive avec try_recv
        // - try_recv() ne bloque pas (contrairement à recv())
        // - Ok(result) : traite le résultat
        // - Err(TryRecvError::Empty) : pas de résultat, continue
        // - Err(TryRecvError::Disconnected) : worker mort (erreur)
        match result_rx.try_recv() {
            Ok(result) => {
                match result {
                    AppResult::TickerDataLoaded { index, data } => {
                        let mut app_lock = app.lock().unwrap();
                        if let Some(item) = app_lock.watchlist.get_mut(index) {
                            info!(ticker = %item.symbol, interval = %data.interval.label(), candles = data.len(), "Updating watchlist item with new data");
                            item.data = Some(data);
                        }
                    }
                    AppResult::LoadError { index: _, symbol, error } => {
                        error!(ticker = %symbol, error = %error, "Failed to load ticker data");
                        // Optionally: show error to user via app state
                    }
                    AppResult::TickerAdded { symbol, name, data } => {
                        let mut app_lock = app.lock().unwrap();
                        info!(ticker = %symbol, candles = data.len(), "Adding ticker to watchlist");
                        // Crée un nouveau WatchlistItem avec les données
                        let item = WatchlistItem::with_data(symbol, name, data);
                        app_lock.watchlist.push(item);
                    }
                    AppResult::AddError { symbol, error } => {
                        error!(ticker = %symbol, error = %error, "Failed to add ticker");
                        // Optionally: show error to user via app state
                    }
                }
            }
            Err(mpsc::TryRecvError::Empty) => {
                // Pas de résultat, c'est normal
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                error!("Worker thread disconnected!");
                // Continue quand même, mais le worker est mort
            }
        }

        // ========================================
        // 1. RENDER : Dessine l'interface
        // ========================================
        // CONCEPT RUST : Closure avec clone d'Arc
        // - Clone l'Arc pour la closure
        // - Lock à l'intérieur de la closure
        // - Unlock automatique à la fin de la closure
        {
            let app_clone = app.clone();
            terminal.draw(|frame| {
                let app_lock = app_clone.lock().unwrap();
                render(frame, &app_lock);
            })?;
        }

        // ========================================
        // 2. INPUT : Traite les événements
        // ========================================
        match events.next() {
            Ok(event) => {
                let mut app_lock = app.lock().unwrap();
                handle_event(&mut app_lock, event, &command_tx);
            }
            Err(_) => {
                // Erreur lors de la lecture d'événement
            }
        }

        // ========================================
        // 3. UPDATE : Met à jour l'état
        // ========================================
        {
            let mut app_lock = app.lock().unwrap();
            app_lock.tick();
        }
    }

    Ok(())
}

// ============================================================================
// Gestion des événements
// ============================================================================
// CONCEPT : Event Handler Pattern
// - Sépare la logique de gestion des événements
// - Modifie l'état de app selon l'événement
// ============================================================================

/// Traite un événement et met à jour l'état de l'application
///
/// CONCEPT RUST : Pattern matching complexe avec guards
/// - Guard clauses (if) pour filtrer les événements
/// - Combinaison de conditions pour gérer différents contextes
/// - Navigation contextuelle selon l'écran actuel
/// - command_tx : pour envoyer des commandes au worker thread
fn handle_event(app: &mut App, event: lazywallet::ui::events::Event, command_tx: &mpsc::Sender<AppCommand>) {
    // Importe les helpers pour vérifier les événements
    use lazywallet::ui::events::{
        get_char_from_event, is_add_event, is_backspace_event, is_delete_event, is_down_event,
        is_enter_event, is_escape_event, is_next_interval_event, is_previous_interval_event,
        is_quit_event, is_space_event, is_ticker_char_event, is_up_event, Event,
    };

    match event {
        Event::Key(_) if is_quit_event(&event) => {
            // Touche 'q' : quit confirmation two-step
            // CONCEPT : Two-step confirmation pour éviter les quits accidentels
            // - Première pression : active confirm_quit
            // - Deuxième pression : quit réel
            if app.is_awaiting_quit_confirmation() {
                info!("User confirmed quit");
                app.quit();
            } else {
                info!("User requested quit (awaiting confirmation)");
                app.request_quit();
            }
        }

        // 'd' : supprimer le ticker sélectionné (seulement sur Dashboard)
        Event::Key(_) if is_delete_event(&event) && app.is_on_dashboard() => {
            // CONCEPT : Two-step delete confirmation (Vim-like)
            // - Première pression : demande confirmation
            // - Deuxième pression : suppression réelle
            if !app.watchlist.is_empty() {
                if app.is_awaiting_delete_confirmation() {
                    // Deuxième pression : on supprime
                    let symbol = app.watchlist.get(app.selected_index)
                        .map(|item| item.symbol.clone())
                        .unwrap_or_default();
                    info!(ticker = %symbol, "User confirmed delete");
                    app.delete_selected();
                } else {
                    // Première pression : on demande confirmation
                    info!("User requested delete (awaiting confirmation)");
                    app.request_delete();
                }
            }
        }

        // 'a' : ajouter un ticker (seulement sur Dashboard)
        Event::Key(_) if is_add_event(&event) && app.is_on_dashboard() => {
            // CONCEPT : Enter input mode (Vim-like)
            // - Change l'écran vers InputMode
            // - Prépare le prompt pour saisir le ticker
            info!("User requested add ticker");
            app.start_input("Add ticker: ".to_string());
        }

        // Navigation dans la watchlist (seulement sur Dashboard)
        Event::Key(_) if is_up_event(&event) && app.is_on_dashboard() => {
            app.cancel_quit(); // Annule les confirmations si actives
            app.cancel_delete();
            debug!("User navigated up");
            app.navigate_up();
        }
        Event::Key(_) if is_down_event(&event) && app.is_on_dashboard() => {
            app.cancel_quit(); // Annule les confirmations si actives
            app.cancel_delete();
            debug!("User navigated down");
            app.navigate_down();
        }

        // Enter : afficher le graphique du ticker sélectionné
        Event::Key(_) if is_enter_event(&event) && app.is_on_dashboard() => {
            app.cancel_quit(); // Annule les confirmations si actives
            app.cancel_delete();
            // CONCEPT : State transition
            // Dashboard → ChartView
            if let Some(item) = app.watchlist.get(app.selected_index) {
                info!(ticker = %item.symbol, "User opened chart view");
            }
            app.show_chart();
        }

        // ESC ou SPACE : retour au dashboard depuis ChartView
        Event::Key(_) if (is_escape_event(&event) || is_space_event(&event)) && app.is_on_chart() => {
            app.cancel_quit(); // Annule la confirmation de quit si active
            // CONCEPT : State transition
            // ChartView → Dashboard
            debug!("User returned to dashboard");
            app.show_dashboard();
        }

        // ========================================
        // Input Mode : Gestion de la saisie
        // ========================================

        // ESC : annuler le mode input
        Event::Key(_) if is_escape_event(&event) && app.is_in_input_mode() => {
            info!("User cancelled input");
            app.cancel_input();
        }

        // Enter : valider le mode input et ajouter le ticker
        Event::Key(_) if is_enter_event(&event) && app.is_in_input_mode() => {
            let symbol = app.submit_input().trim().to_uppercase();
            if !symbol.is_empty() {
                info!(ticker = %symbol, "User submitted ticker for adding");
                // Envoie la commande au worker pour ajouter le ticker
                let _ = command_tx.send(AppCommand::AddTicker { symbol });
            } else {
                debug!("Empty ticker symbol, ignoring");
            }
        }

        // Backspace : supprimer le dernier caractère
        Event::Key(_) if is_backspace_event(&event) && app.is_in_input_mode() => {
            app.backspace();
        }

        // Caractères : ajouter au buffer
        Event::Key(_) if is_ticker_char_event(&event) && app.is_in_input_mode() => {
            if let Some(c) = get_char_from_event(&event) {
                app.append_char(c);
            }
        }

        // 'l' : intervalle suivant (seulement sur ChartView)
        Event::Key(_) if is_next_interval_event(&event) && app.is_on_chart() => {
            app.cancel_quit(); // Annule la confirmation de quit si active
            app.next_interval();
            info!(interval = %app.current_interval.label(), "User changed to next interval");

            // Envoie la commande de rechargement au worker
            if let Some(item) = app.watchlist.get(app.selected_index) {
                let _ = command_tx.send(AppCommand::ReloadTickerData {
                    symbol: item.symbol.clone(),
                    interval: app.current_interval,
                    index: app.selected_index,
                });
            }
        }

        // 'h' : intervalle précédent (seulement sur ChartView)
        Event::Key(_) if is_previous_interval_event(&event) && app.is_on_chart() => {
            app.cancel_quit(); // Annule la confirmation de quit si active
            app.previous_interval();
            info!(interval = %app.current_interval.label(), "User changed to previous interval");

            // Envoie la commande de rechargement au worker
            if let Some(item) = app.watchlist.get(app.selected_index) {
                let _ = command_tx.send(AppCommand::ReloadTickerData {
                    symbol: item.symbol.clone(),
                    interval: app.current_interval,
                    index: app.selected_index,
                });
            }
        }

        Event::Tick => {
            // Tick régulier : rien à faire pour l'instant
        }

        Event::Key(_) => {
            // Toute autre touche : annule les confirmations si actives
            app.cancel_quit();
            app.cancel_delete();
        }

        _ => {
            // Autres événements : ignorés
        }
    }
}

// ============================================================================
// Setup et restauration du terminal
// ============================================================================
// CONCEPT RUST : Terminal raw mode
// - Raw mode : on reçoit tous les caractères directement
// - Alternate screen : écran secondaire (ne pollue pas l'historique)
// - Crossterm gère tout ça de manière cross-platform
//
// IMPORTANT : Toujours restaurer le terminal avant de quitter !
// ============================================================================

/// Configure le terminal en mode TUI
///
/// CONCEPT RUST : Error propagation avec ?
/// - Chaque opération peut échouer
/// - ? propage automatiquement les erreurs
/// - Type de retour : Result<Terminal<...>>
fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    // Active le raw mode
    // CONCEPT : Raw mode
    // - Les caractères ne sont pas affichés automatiquement
    // - Pas de buffering ligne par ligne
    // - Contrôle total sur l'affichage
    enable_raw_mode()?;

    // Configure le terminal
    // CONCEPT : Alternate screen
    // - Écran secondaire qui ne pollue pas l'historique
    // - Quand on quitte, l'écran précédent est restauré
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture  // Active la souris (optionnel)
    )?;

    // Crée le backend crossterm
    let backend = CrosstermBackend::new(stdout);

    // Crée le terminal ratatui
    // CONCEPT RUST : Ownership
    // - Terminal prend ownership de backend
    // - On retourne le Terminal
    Terminal::new(backend).map_err(|e| e.into())
}

/// Restaure le terminal à son état normal
///
/// CONCEPT : Cleanup et RAII
/// - Appelé dans main() même en cas d'erreur
/// - Restaure le terminal pour ne pas le laisser cassé
fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    // Désactive le raw mode
    disable_raw_mode()?;

    // Restaure le terminal
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;

    // Affiche le curseur
    terminal.show_cursor()?;

    Ok(())
}

// ============================================================================
// Notes pédagogiques
// ============================================================================
//
// NOUVEAUX CONCEPTS RUST APPRIS :
//
// 1. Terminal raw mode et TUI
//    - enable_raw_mode() : contrôle total du terminal
//    - Alternate screen : écran temporaire
//    - Restauration obligatoire
//
// 2. Event Loop pattern
//    - Loop infinie : while app.is_running()
//    - Render → Input → Update
//    - Pattern classique des jeux et apps interactives
//
// 3. Closures
//    - |frame| { ... } : fonction anonyme
//    - Capture des variables
//    - Passée à terminal.draw()
//
// 4. Pattern matching avancé
//    - Match sur enums avec données
//    - Guards : if is_quit_event(&event)
//    - Exhaustivité vérifiée par le compilateur
//
// 5. RAII et cleanup
//    - Acquisition dans setup_terminal()
//    - Libération dans restore_terminal()
//    - Même en cas d'erreur (important!)
//
// PROCHAINES ÉTAPES (Phase 2 Étape 2) :
// - Ajouter une watchlist de tickers
// - Navigation ↑↓ au clavier
// - Affichage des prix avec couleurs
// - Rafraîchissement automatique
//
// ============================================================================
