// ============================================================================
// Structure : OHLC (Open, High, Low, Close)
// ============================================================================
// Représente les données d'une chandelle japonaise (candlestick)
//
// CONCEPTS RUST :
// 1. DateTime<Utc> : type de chrono pour dates avec timezone UTC
// 2. FixedOffset : décalage horaire fixe (ex: UTC-4 pour New York en été)
// 3. f64 : floating point 64 bits pour les prix (précision suffisante)
// 4. u64 : unsigned 64 bits pour le volume (toujours positif)
// ============================================================================

use chrono::{DateTime, FixedOffset, Utc};

/// Intervalle de temps entre les chandelles
///
/// CONCEPT : L'intervalle fixe la granularité des chandelles (5m, 1h, 1d...)
/// et la profondeur d'historique demandée à Yahoo (`history_days`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interval {
    /// 5 minutes
    M5,
    /// 15 minutes
    M15,
    /// 30 minutes
    M30,
    /// 1 heure
    H1,
    /// 4 heures
    H4,
    /// 1 jour (daily)
    D1,
    /// 1 semaine (weekly)
    W1,
}

impl Interval {
    /// Convertit l'intervalle en string pour l'API Yahoo Finance
    ///
    /// CONCEPT RUST : &'static str
    /// - Retourne une string littérale (dans le binaire)
    /// - Lifetime 'static : vit pendant toute l'exécution
    pub fn to_yahoo_string(self) -> &'static str {
        match self {
            Interval::M5 => "5m",
            Interval::M15 => "15m",
            Interval::M30 => "30m",
            Interval::H1 => "1h",
            Interval::H4 => "4h",
            Interval::D1 => "1d",
            Interval::W1 => "1wk",
        }
    }

    /// Retourne le label court pour l'affichage
    pub fn label(self) -> &'static str {
        match self {
            Interval::M5 => "5m",
            Interval::M15 => "15m",
            Interval::M30 => "30m",
            Interval::H1 => "1h",
            Interval::H4 => "4h",
            Interval::D1 => "1d",
            Interval::W1 => "1w",
        }
    }

    /// Jours d'historique demandés à Yahoo
    ///
    /// CONCEPT : Assez de chandeliers pour remplir un terminal large (~500 pour une action,
    /// qui ne cote que 6h30 par jour), en restant sous la limite Yahoo de 60 jours en intraday.
    pub fn history_days(self) -> i64 {
        match self {
            Interval::M5 => 14,
            Interval::M15 => 30,
            Interval::M30 => 58,
            Interval::H1 => 180,
            Interval::H4 => 365,
            Interval::D1 => 730,
            Interval::W1 => 3650,
        }
    }

    /// Retourne l'intervalle suivant (cycle)
    ///
    /// CONCEPT RUST : `self` par valeur
    /// - Interval est Copy : le prendre par valeur ne coûte rien
    /// - Permet d'utiliser `Interval::next` comme `fn(Interval) -> Interval`
    pub fn next(self) -> Interval {
        match self {
            Interval::M5 => Interval::M15,
            Interval::M15 => Interval::M30,
            Interval::M30 => Interval::H1,
            Interval::H1 => Interval::H4,
            Interval::H4 => Interval::D1,
            Interval::D1 => Interval::W1,
            Interval::W1 => Interval::M5, // Boucle
        }
    }

    /// Retourne l'intervalle précédent (cycle)
    pub fn previous(self) -> Interval {
        match self {
            Interval::M5 => Interval::W1, // Boucle
            Interval::M15 => Interval::M5,
            Interval::M30 => Interval::M15,
            Interval::H1 => Interval::M30,
            Interval::H4 => Interval::H1,
            Interval::D1 => Interval::H4,
            Interval::W1 => Interval::D1,
        }
    }
}

impl Default for Interval {
    /// Intervalle par défaut : 30 minutes (bon équilibre détail/contexte)
    fn default() -> Self {
        Interval::M30
    }
}

/// Une chandelle japonaise (candlestick)
#[derive(Debug, Clone)]
pub struct OHLC {
    /// Timestamp de la chandelle (UTC)
    pub timestamp: DateTime<Utc>,
    /// Prix d'ouverture (Open)
    pub open: f64,
    /// Prix le plus haut (High)
    pub high: f64,
    /// Prix le plus bas (Low)
    pub low: f64,
    /// Prix de clôture (Close)
    pub close: f64,
    /// Volume échangé
    pub volume: u64,
}

impl OHLC {
    /// Constructeur : crée une nouvelle chandelle OHLC
    pub fn new(
        timestamp: DateTime<Utc>,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: u64,
    ) -> Self {
        Self {
            timestamp,
            open,
            high,
            low,
            close,
            volume,
        }
    }

    /// Chandelle haussière (ou neutre : un doji compte comme hausse)
    pub fn is_bullish(&self) -> bool {
        self.close >= self.open
    }
}

/// Collection de chandelles OHLC pour un ticker
///
/// CONCEPT RUST : Vec<T>
/// - Vec est un tableau dynamique (growable array)
/// - Stocké sur le heap, peut grandir/rétrécir
#[derive(Debug, Clone)]
pub struct OHLCData {
    /// Symbole du ticker
    pub symbol: String,

    /// Intervalle entre les chandelles (5m, 30m, 1h, 1d, etc.)
    pub interval: Interval,

    /// Décalage horaire de la place de cotation (New York : UTC-4 en été)
    ///
    /// ponytail: décalage fixe pris au moment du fetch ; un changement d'heure
    /// dans la fenêtre décale d'1h les labels intraday les plus anciens.
    /// Évolution : chrono-tz avec `meta.exchangeTimezoneName`.
    pub utc_offset: FixedOffset,

    /// Liste des chandelles, triées par timestamp croissant
    pub candles: Vec<OHLC>,
}

impl OHLCData {
    /// Crée une collection vide
    pub fn new(symbol: String, interval: Interval, utc_offset: FixedOffset) -> Self {
        Self {
            symbol,
            interval,
            utc_offset,
            candles: Vec::new(),
        }
    }

    /// Ajoute une chandelle
    pub fn add_candle(&mut self, candle: OHLC) {
        self.candles.push(candle);
    }

    /// Retourne le nombre de chandelles
    pub fn len(&self) -> usize {
        self.candles.len()
    }

    /// Vérifie si la collection est vide
    pub fn is_empty(&self) -> bool {
        self.candles.is_empty()
    }

    /// Retourne la chandelle la plus récente
    pub fn last(&self) -> Option<&OHLC> {
        self.candles.last()
    }

    /// Heure de la chandelle à l'heure de la place
    pub fn local_time(&self, candle: &OHLC) -> DateTime<FixedOffset> {
        candle.timestamp.with_timezone(&self.utc_offset)
    }

    /// Clôture de la dernière séance avant celle de la chandelle la plus récente
    ///
    /// CONCEPT : Une "séance" = une date à l'heure de la place, pas en UTC.
    /// None en W1 : une chandelle couvre plusieurs séances.
    pub fn previous_session_close(&self) -> Option<f64> {
        if self.interval == Interval::W1 {
            return None;
        }
        let last_day = self.local_time(self.last()?).date_naive();
        self.candles
            .iter()
            .rev()
            .find(|c| self.local_time(c).date_naive() < last_day)
            .map(|c| c.close)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(offset: FixedOffset, y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
        offset
            .with_ymd_and_hms(y, m, d, h, min, 0)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn test_ohlc_bullish() {
        let ohlc = OHLC::new(Utc::now(), 100.0, 110.0, 95.0, 105.0, 1000);
        assert!(ohlc.is_bullish());
        // Doji (close == open) : compté comme haussier, comme dans le graphique
        assert!(OHLC::new(Utc::now(), 100.0, 101.0, 99.0, 100.0, 0).is_bullish());
        assert!(!OHLC::new(Utc::now(), 100.0, 105.0, 90.0, 95.0, 0).is_bullish());
    }

    #[test]
    fn test_interval_yahoo_string() {
        assert_eq!(Interval::M30.to_yahoo_string(), "30m");
        assert_eq!(Interval::H1.to_yahoo_string(), "1h");
        assert_eq!(Interval::D1.to_yahoo_string(), "1d");
        assert_eq!(Interval::W1.to_yahoo_string(), "1wk");
    }

    #[test]
    fn test_interval_cycle() {
        assert_eq!(Interval::M5.next(), Interval::M15);
        assert_eq!(Interval::M5.previous(), Interval::W1);
        assert_eq!(Interval::W1.next(), Interval::M5); // Boucle
    }

    #[test]
    fn previous_session_close_uses_exchange_timezone() {
        // New York (UTC-4) : la chandelle de 22:00 locale est le 21/09 en NY mais le 22/09 en UTC
        let ny = FixedOffset::west_opt(4 * 3600).unwrap();
        let mut data = OHLCData::new("AAPL".into(), Interval::M30, ny);
        data.add_candle(OHLC::new(
            at(ny, 2026, 9, 21, 15, 30),
            100.0,
            101.0,
            99.0,
            100.5,
            0,
        ));
        data.add_candle(OHLC::new(
            at(ny, 2026, 9, 21, 22, 0),
            100.5,
            102.0,
            100.0,
            101.5,
            0,
        ));
        data.add_candle(OHLC::new(
            at(ny, 2026, 9, 22, 9, 30),
            101.5,
            104.0,
            101.0,
            103.0,
            0,
        ));
        assert_eq!(data.previous_session_close(), Some(101.5));
    }

    #[test]
    fn previous_session_close_daily_and_weekly() {
        let utc = FixedOffset::east_opt(0).unwrap();
        let mut daily = OHLCData::new("BTC-USD".into(), Interval::D1, utc);
        daily.add_candle(OHLC::new(
            at(utc, 2026, 9, 21, 0, 0),
            1.0,
            1.0,
            1.0,
            10.0,
            0,
        ));
        daily.add_candle(OHLC::new(
            at(utc, 2026, 9, 22, 0, 0),
            10.0,
            12.0,
            9.0,
            11.0,
            0,
        ));
        assert_eq!(daily.previous_session_close(), Some(10.0));
        let mut weekly = daily.clone();
        weekly.interval = Interval::W1;
        // Une chandelle hebdo couvre plusieurs séances : pas de clôture de veille fiable
        assert_eq!(weekly.previous_session_close(), None);
    }

    #[test]
    fn history_days_stay_within_yahoo_limits() {
        // Yahoo refuse l'intraday (< 1h) au-delà de 60 jours
        for i in [Interval::M5, Interval::M15, Interval::M30] {
            assert!(i.history_days() < 60);
        }
        assert_eq!(Interval::W1.history_days(), 3650);
    }
}
