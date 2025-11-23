// ============================================================================
// Structure : OHLC (Open, High, Low, Close)
// ============================================================================
// Représente les données d'une chandelle japonaise (candlestick)
//
// CONCEPTS RUST :
// 1. DateTime<Utc> : type de chrono pour dates avec timezone UTC
// 2. f64 : floating point 64 bits pour les prix (précision suffisante)
// 3. u64 : unsigned 64 bits pour le volume (toujours positif)
// ============================================================================

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Période de temps pour les données OHLC
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Timeframe {
    /// 1 jour de données
    OneDay,
    /// 7 jours de données
    OneWeek,
    /// 1 mois (30 jours)
    OneMonth,
    /// 3 mois
    ThreeMonths,
    /// 6 mois
    SixMonths,
    /// 1 an
    OneYear,
}

impl Timeframe {
    /// Retourne le nombre de jours correspondant
    pub fn to_days(&self) -> u32 {
        match self {
            Timeframe::OneDay => 1,
            Timeframe::OneWeek => 7,
            Timeframe::OneMonth => 30,
            Timeframe::ThreeMonths => 90,
            Timeframe::SixMonths => 180,
            Timeframe::OneYear => 365,
        }
    }

    /// Retourne le label pour l'affichage
    pub fn label(&self) -> &str {
        match self {
            Timeframe::OneDay => "1D",
            Timeframe::OneWeek => "7D",
            Timeframe::OneMonth => "1M",
            Timeframe::ThreeMonths => "3M",
            Timeframe::SixMonths => "6M",
            Timeframe::OneYear => "1Y",
        }
    }
}

/// Intervalle de temps entre les chandelles
///
/// CONCEPT : Intervalle vs Timeframe
/// - Interval : granularité des chandelles (5m, 30m, 1h, 1d, etc.)
/// - Timeframe : période totale affichée (7 jours, 1 mois, etc.)
/// - Relation : interval détermine le timeframe par défaut
///
/// Exemples :
/// - M5 (5 minutes) → affiche 7 jours
/// - M30 (30 minutes) → affiche 14 jours
/// - D1 (1 jour) → affiche 6 mois
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    /// - Pas d'allocation, très efficace
    pub fn to_yahoo_string(&self) -> &'static str {
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
    pub fn label(&self) -> &'static str {
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

    /// Retourne le timeframe par défaut pour cet intervalle
    ///
    /// CONCEPT : Mapping interval → timeframe
    /// Basé sur les pratiques standards de Yahoo Finance et TradingView :
    /// - Intraday court (1m, 5m) : 1-5 jours
    /// - Intraday moyen (15m, 30m) : 7-14 jours
    /// - Intraday long (1h, 4h) : 30-90 jours
    /// - Daily+ (1d, 1w) : 180-365 jours
    ///
    /// Limitations Yahoo Finance :
    /// - 1m : max 7 jours de données disponibles
    /// - Intraday (<1d) : max 60 jours
    pub fn default_timeframe(&self) -> Timeframe {
        match self {
            Interval::M5 => Timeframe::OneWeek,       // 5m : 7 jours
            Interval::M15 => Timeframe::OneWeek,      // 15m : 7 jours
            Interval::M30 => Timeframe::OneMonth,     // 30m : 30 jours (au lieu de 14j)
            Interval::H1 => Timeframe::OneMonth,      // 1h : 30 jours
            Interval::H4 => Timeframe::ThreeMonths,   // 4h : 90 jours
            Interval::D1 => Timeframe::SixMonths,     // 1d : 180 jours
            Interval::W1 => Timeframe::OneYear,       // 1w : 365 jours
        }
    }

    /// Format de label pour l'axe X selon l'intervalle
    ///
    /// CONCEPT : Formatage adaptatif
    /// - Intraday : affiche l'heure (HH:MM)
    /// - Daily+ : affiche la date (DD/MM)
    ///
    /// Retourne un tuple (format_chrono, est_intraday)
    /// - format_chrono : string de format pour chrono (ex: "%H:%M")
    /// - est_intraday : true si besoin d'afficher les heures
    pub fn x_axis_format(&self) -> (&'static str, bool) {
        match self {
            Interval::M5 | Interval::M15 | Interval::M30 | Interval::H1 => {
                ("%H:%M", true) // Format heure:minute pour intraday
            }
            Interval::H4 => {
                ("%d/%m %H:00", true) // Date + heure pour 4h
            }
            Interval::D1 => {
                ("%d/%m", false) // Date seulement pour daily
            }
            Interval::W1 => {
                ("%d %b", false) // Jour + mois abrégé pour weekly
            }
        }
    }

    /// Retourne tous les intervalles disponibles (pour UI de sélection)
    pub fn all() -> Vec<Interval> {
        vec![
            Interval::M5,
            Interval::M15,
            Interval::M30,
            Interval::H1,
            Interval::H4,
            Interval::D1,
            Interval::W1,
        ]
    }

    /// Retourne l'intervalle suivant (cycle)
    pub fn next(&self) -> Interval {
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
    pub fn previous(&self) -> Interval {
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
///
/// CONCEPT RUST : Struct avec lifetime
/// - Pour l'instant, pas de lifetime car on possède toutes les données
/// - DateTime<Utc> est "owned" (possède ses données)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OHLC {
    /// Timestamp de la chandelle
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

    /// Vérifie si la chandelle est haussière (bullish)
    /// CONCEPT RUST : &self (référence immutable)
    /// - Ne modifie pas l'objet
    /// - Pas de copie, juste une référence
    pub fn is_bullish(&self) -> bool {
        self.close > self.open
    }

    /// Vérifie si la chandelle est baissière (bearish)
    pub fn is_bearish(&self) -> bool {
        self.close < self.open
    }

    /// Calcule le corps de la chandelle (body)
    pub fn body(&self) -> f64 {
        (self.close - self.open).abs()
    }

    /// Calcule la mèche haute (upper wick)
    pub fn upper_wick(&self) -> f64 {
        self.high - self.open.max(self.close)
    }

    /// Calcule la mèche basse (lower wick)
    pub fn lower_wick(&self) -> f64 {
        self.open.min(self.close) - self.low
    }

    /// Variation en pourcentage depuis l'ouverture
    pub fn change_percent(&self) -> f64 {
        if self.open == 0.0 {
            0.0
        } else {
            ((self.close - self.open) / self.open) * 100.0
        }
    }
}

/// Collection de chandelles OHLC pour un ticker
///
/// CONCEPT RUST : Vec<T>
/// - Vec est un tableau dynamique (growable array)
/// - Stocké sur le heap, peut grandir/rétrécir
/// - Équivalent de std::vector en C++ ou ArrayList en Java
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OHLCData {
    /// Symbole du ticker
    pub symbol: String,

    /// Intervalle entre les chandelles (1m, 30m, 1h, 1d, etc.)
    pub interval: Interval,

    /// Période de temps totale affichée
    pub timeframe: Timeframe,

    /// Liste des chandelles, triées par timestamp croissant
    /// CONCEPT RUST : Ownership
    /// - OHLCData possède le Vec
    /// - Le Vec possède tous les OHLC
    /// - Quand OHLCData est drop, tout est libéré automatiquement
    pub candles: Vec<OHLC>,
}

impl OHLCData {
    /// Crée une nouvelle collection OHLC vide avec interval et timeframe spécifiques
    pub fn new(symbol: String, interval: Interval, timeframe: Timeframe) -> Self {
        Self {
            symbol,
            interval,
            timeframe,
            candles: Vec::new(),
        }
    }

    /// Crée une nouvelle collection OHLC avec interval et timeframe par défaut de l'interval
    ///
    /// CONCEPT : Constructor convenience
    /// - Simplifie la création quand on veut utiliser le timeframe par défaut
    /// - L'interval détermine automatiquement le timeframe optimal
    pub fn with_interval(symbol: String, interval: Interval) -> Self {
        let timeframe = interval.default_timeframe();
        Self::new(symbol, interval, timeframe)
    }

    /// Ajoute une chandelle
    ///
    /// CONCEPT RUST : mut self
    /// - Méthode qui modifie l'objet
    /// - Nécessite que l'appelant ait une référence mutable
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
    ///
    /// CONCEPT RUST : Option<&OHLC>
    /// - Retourne une référence à la dernière chandelle
    /// - Option car peut être vide
    /// - & car on ne veut pas donner ownership
    pub fn last(&self) -> Option<&OHLC> {
        self.candles.last()
    }

    /// Calcule le prix minimum sur toute la période
    pub fn min_price(&self) -> Option<f64> {
        self.candles
            .iter()  // Crée un itérateur
            .map(|c| c.low)  // Transforme chaque OHLC en son prix bas
            .min_by(|a, b| a.partial_cmp(b).unwrap())  // Trouve le minimum
    }

    /// Calcule le prix maximum sur toute la période
    pub fn max_price(&self) -> Option<f64> {
        self.candles
            .iter()
            .map(|c| c.high)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
    }

    /// Calcule la variation totale en pourcentage
    ///
    /// CONCEPT RUST : Pattern matching avec if let
    /// - Équivalent à un if avec destructuration
    /// - Plus ergonomique que match pour un seul cas
    pub fn total_change_percent(&self) -> Option<f64> {
        if let (Some(first), Some(last)) = (self.candles.first(), self.candles.last()) {
            if first.open == 0.0 {
                return None;
            }
            Some(((last.close - first.open) / first.open) * 100.0)
        } else {
            None
        }
    }

    /// Calcule la variation journalière en pourcentage
    ///
    /// CONCEPT : Daily change calculation
    /// - Pour intervalles D1/W1 : variation de la dernière chandelle
    /// - Pour intervalles intraday : variation du dernier jour avec données
    /// - Gère les marchés fermés (utilise la dernière journée disponible)
    ///
    /// Algorithme :
    /// 1. Si D1 ou W1 : chaque chandelle = 1 jour/semaine → utiliser change_percent()
    /// 2. Si intraday : trouver toutes les chandelles du dernier jour
    /// 3. Calculer : ((close_du_jour - open_du_jour) / open_du_jour) * 100
    pub fn daily_change_percent(&self) -> Option<f64> {
        if self.candles.is_empty() {
            return None;
        }

        // Pour les intervalles daily et weekly, la chandelle représente déjà une journée/semaine
        if matches!(self.interval, Interval::D1 | Interval::W1) {
            return self.last().map(|c| c.change_percent());
        }

        // Pour les intervalles intraday (M5, M15, M30, H1, H4)
        // Trouver toutes les chandelles du dernier jour disponible
        let last_candle = self.last()?;
        let last_date = last_candle.timestamp.date_naive();

        // Filtrer les chandelles du même jour
        let day_candles: Vec<&OHLC> = self
            .candles
            .iter()
            .filter(|c| c.timestamp.date_naive() == last_date)
            .collect();

        if day_candles.is_empty() {
            return None;
        }

        // Open de la première chandelle du jour, Close de la dernière
        let day_open = day_candles.first()?.open;
        let day_close = day_candles.last()?.close;

        if day_open == 0.0 {
            return None;
        }

        Some(((day_close - day_open) / day_open) * 100.0)
    }
}

// ============================================================================
// Tests unitaires
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_ohlc_bullish() {
        let ohlc = OHLC::new(Utc::now(), 100.0, 110.0, 95.0, 105.0, 1000);
        assert!(ohlc.is_bullish());
        assert!(!ohlc.is_bearish());
    }

    #[test]
    fn test_ohlc_bearish() {
        let ohlc = OHLC::new(Utc::now(), 100.0, 105.0, 90.0, 95.0, 1000);
        assert!(ohlc.is_bearish());
        assert!(!ohlc.is_bullish());
    }

    #[test]
    fn test_ohlc_data() {
        let mut data = OHLCData::new("AAPL".to_string(), Interval::M30, Timeframe::OneWeek);

        assert!(data.is_empty());

        data.add_candle(OHLC::new(Utc::now(), 100.0, 110.0, 95.0, 105.0, 1000));
        data.add_candle(OHLC::new(Utc::now(), 105.0, 115.0, 100.0, 110.0, 1200));

        assert_eq!(data.len(), 2);
        assert!(!data.is_empty());
    }

    #[test]
    fn test_timeframe_to_days() {
        assert_eq!(Timeframe::OneDay.to_days(), 1);
        assert_eq!(Timeframe::OneWeek.to_days(), 7);
        assert_eq!(Timeframe::OneYear.to_days(), 365);
    }

    #[test]
    fn test_interval_yahoo_string() {
        assert_eq!(Interval::M30.to_yahoo_string(), "30m");
        assert_eq!(Interval::H1.to_yahoo_string(), "1h");
        assert_eq!(Interval::D1.to_yahoo_string(), "1d");
        assert_eq!(Interval::W1.to_yahoo_string(), "1wk");
    }

    #[test]
    fn test_interval_default_timeframe() {
        assert_eq!(Interval::M30.default_timeframe(), Timeframe::OneMonth);
        assert_eq!(Interval::D1.default_timeframe(), Timeframe::SixMonths);
        assert_eq!(Interval::W1.default_timeframe(), Timeframe::OneYear);
    }

    #[test]
    fn test_interval_cycle() {
        assert_eq!(Interval::M5.next(), Interval::M15);
        assert_eq!(Interval::M5.previous(), Interval::W1);
        assert_eq!(Interval::W1.next(), Interval::M5); // Boucle
    }

    #[test]
    fn test_ohlcdata_with_interval() {
        let data = OHLCData::with_interval("BTC-USD".to_string(), Interval::H1);
        assert_eq!(data.symbol, "BTC-USD");
        assert_eq!(data.interval, Interval::H1);
        assert_eq!(data.timeframe, Timeframe::OneMonth); // Default pour H1
    }

    #[test]
    fn test_daily_change_percent_d1() {
        // Pour D1, chaque chandelle = 1 journée
        let mut data = OHLCData::new("AAPL".to_string(), Interval::D1, Timeframe::OneWeek);

        // Ajoute une chandelle avec open=100 et close=105 (hausse de 5%)
        data.add_candle(OHLC::new(Utc::now(), 100.0, 110.0, 95.0, 105.0, 1000));

        let change = data.daily_change_percent();
        assert!(change.is_some());
        assert_eq!(change.unwrap(), 5.0);
    }

    #[test]
    fn test_daily_change_percent_intraday() {
        use chrono::{Duration, TimeZone};

        // Pour M30, on a plusieurs chandelles dans la journée
        let mut data = OHLCData::new("AAPL".to_string(), Interval::M30, Timeframe::OneWeek);

        let today = Utc::now().date_naive();
        let base_time = Utc.from_utc_datetime(&today.and_hms_opt(9, 0, 0).unwrap());

        // Première chandelle du jour : open=100
        data.add_candle(OHLC::new(base_time, 100.0, 102.0, 99.0, 101.0, 1000));

        // Chandelles intermédiaires
        data.add_candle(OHLC::new(
            base_time + Duration::minutes(30),
            101.0,
            103.0,
            100.0,
            102.0,
            1100,
        ));

        // Dernière chandelle du jour : close=105
        data.add_candle(OHLC::new(
            base_time + Duration::hours(1),
            102.0,
            105.0,
            101.0,
            105.0,
            1200,
        ));

        // Variation journalière = (105 - 100) / 100 = 5%
        let change = data.daily_change_percent();
        assert!(change.is_some());
        assert_eq!(change.unwrap(), 5.0);
    }

    #[test]
    fn test_daily_change_percent_multiple_days() {
        use chrono::{Duration, TimeZone};

        // Données intraday sur plusieurs jours
        let mut data = OHLCData::new("AAPL".to_string(), Interval::H1, Timeframe::OneWeek);

        let today = Utc::now().date_naive();
        let yesterday = today - Duration::days(1);

        let yesterday_time = Utc.from_utc_datetime(&yesterday.and_hms_opt(9, 0, 0).unwrap());
        let today_time = Utc.from_utc_datetime(&today.and_hms_opt(9, 0, 0).unwrap());

        // Hier : de 100 à 110 (hausse de 10%)
        data.add_candle(OHLC::new(yesterday_time, 100.0, 105.0, 99.0, 110.0, 1000));

        // Aujourd'hui : de 110 à 115 (hausse de ~4.54%)
        data.add_candle(OHLC::new(today_time, 110.0, 116.0, 109.0, 115.0, 1100));
        data.add_candle(OHLC::new(
            today_time + Duration::hours(1),
            115.0,
            116.0,
            114.0,
            115.0,
            1200,
        ));

        // Devrait calculer uniquement la variation d'aujourd'hui
        // (115 - 110) / 110 = 4.545454...%
        let change = data.daily_change_percent();
        assert!(change.is_some());
        let change_value = change.unwrap();
        assert!((change_value - 4.545454).abs() < 0.001); // Vérification avec tolérance
    }
}
