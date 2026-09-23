// ============================================================================
// LazyWallet - Point d'entrée
// ============================================================================
// Programme TUI : watchlist de tickers et graphiques en chandeliers
//
// CONCEPTS RUST CLÉS :
// 1. Terminal raw mode : contrôle total du terminal
// 2. Event loop : boucle qui traite résultats, rendu et clavier
// 3. Threads + channels : le réseau tourne dans un worker, l'UI ne bloque jamais
// 4. Panic hook : le terminal est restauré même en cas de panic
// ============================================================================

use std::io;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::Instant;

use anyhow::{bail, Context, Result};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tracing::info;

use lazywallet::app::App;
use lazywallet::ui::events::{handle_key, EventHandler};
use lazywallet::ui::render;
use lazywallet::watchlist_file;
use lazywallet::worker::{spawn_worker, AppCommand, AppResult};

// ============================================================================
// Initialisation du logging
// ============================================================================
// CONCEPT : Logging dans une app TUI
// - Les println! ne fonctionnent pas une fois le TUI lancé
// - On log vers un fichier à la place, avec rotation quotidienne
// ============================================================================

/// Initialise le système de logging vers `./logs/lazywallet.log.YYYY-MM-DD`
///
/// ```bash
/// tail -f logs/lazywallet.log.*          # suivre les logs
/// RUST_LOG=lazywallet=trace cargo run    # changer le niveau
/// ```
fn init_logging() -> Result<()> {
    use tracing_appender::rolling::{RollingFileAppender, Rotation};
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    let log_dir = std::path::PathBuf::from("./logs");
    std::fs::create_dir_all(&log_dir).context("Échec de la création du répertoire de logs")?;
    let file_appender =
        RollingFileAppender::new(Rotation::DAILY, log_dir.clone(), "lazywallet.log");

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(file_appender)
                .with_ansi(false)
                .with_target(true)
                .with_thread_ids(true)
                .with_line_number(true),
        )
        .with(
            // RUST_LOG surcharge ; par défaut debug pour lazywallet, info pour le reste
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "lazywallet=debug,info".into()),
        )
        .init();

    info!(?log_dir, "Logging initialisé");
    Ok(())
}

// ============================================================================
// Point d'entrée du programme
// ============================================================================

fn main() -> Result<()> {
    // Sans logging l'app reste utilisable : on prévient et on continue
    init_logging().unwrap_or_else(|e| eprintln!("⚠️  Logging désactivé : {e:#}"));

    let (command_tx, command_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    spawn_worker(command_rx, result_tx)?;

    let path = watchlist_file::default_path()?;
    let entries = watchlist_file::load(&path)?;
    let mut app = App::from_entries(entries, Some(path), Instant::now());

    install_panic_hook();
    let mut terminal = setup_terminal()?;
    let result = run(&mut terminal, &mut app, &command_tx, &result_rx);
    // Restaure le terminal (même si run() a échoué)
    restore_terminal(&mut terminal)?;
    result
}

// ============================================================================
// Event Loop Principal
// ============================================================================
// CONCEPT : À chaque tour
//   1. Intégrer tous les résultats arrivés du worker
//   2. Dessiner l'interface
//   3. Attendre une touche (max 250 ms) et mettre à jour l'état
//   4. Envoyer au worker les commandes produites
// ============================================================================

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    commands: &Sender<AppCommand>,
    results: &Receiver<AppResult>,
) -> Result<()> {
    let events = EventHandler;
    let initial = app.initial_commands();
    send_all(app, commands, initial)?;

    while app.is_running() {
        // Vide tous les résultats arrivés, pas un seul par image
        loop {
            match results.try_recv() {
                Ok(result) => app.apply_result(result, Instant::now()),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => bail!("Le thread réseau s'est arrêté"),
            }
        }

        terminal.draw(|frame| render(frame, app))?;

        let now = Instant::now();
        let mut batch = app.tick(now);
        if let Some(key) = events.next()? {
            batch.extend(handle_key(app, key, now));
        }
        send_all(app, commands, batch)?;
    }

    Ok(())
}

/// Envoie des commandes au worker et les compte comme "en vol"
fn send_all(app: &mut App, commands: &Sender<AppCommand>, batch: Vec<AppCommand>) -> Result<()> {
    for command in batch {
        commands
            .send(command)
            .context("Le thread réseau s'est arrêté")?;
        app.pending += 1;
    }
    Ok(())
}

// ============================================================================
// Setup et restauration du terminal
// ============================================================================
// CONCEPT : Raw mode + alternate screen
// - Raw mode : on reçoit toutes les touches directement
// - Alternate screen : écran secondaire, l'historique du shell est préservé
// IMPORTANT : Toujours restaurer le terminal avant de quitter !
// ============================================================================

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

/// Restaure le terminal avant d'afficher un panic, sinon le message est illisible
///
/// CONCEPT RUST : panic hook
/// - `take_hook()` récupère le hook par défaut (qui imprime le message)
/// - le nouveau hook restaure le terminal puis délègue au hook par défaut
fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Best effort : pendant un panic, une erreur de restauration n'a nulle part où aller
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        default_hook(info);
    }));
}
