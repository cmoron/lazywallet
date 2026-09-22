// ============================================================================
// Axe du temps : choisit le pas de graduation selon la place disponible
// ============================================================================

//
// CONCEPT : Plutôt que des seuils de largeur fixés à la main, on essaie des pas
// du plus fin au plus grossier (5 min → 10 ans) et on garde le premier dont les
// labels tiennent sans se chevaucher. Un label tombe sur la chandelle qui ouvre
// une nouvelle "case" (heure, jour, mois...) : les trous du marché (nuits,
// week-ends) sont gérés d'office, et le décalage horaire de la place est déjà
// dans `DateTime<FixedOffset>`.
// ============================================================================

use chrono::{DateTime, Datelike, FixedOffset};

/// Pas de graduation temporelle
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Minutes(i64),
    Hours(i64),
    Days(i64),
    Weeks(i64),
    Months(i64),
    Years(i64),
}

/// Pas candidats, du plus fin au plus grossier
const LADDER: [Step; 19] = [
    Step::Minutes(5),
    Step::Minutes(15),
    Step::Minutes(30),
    Step::Hours(1),
    Step::Hours(2),
    Step::Hours(3),
    Step::Hours(6),
    Step::Hours(12),
    Step::Days(1),
    Step::Days(2),
    Step::Weeks(1),
    Step::Weeks(2),
    Step::Months(1),
    Step::Months(3),
    Step::Months(6),
    Step::Years(1),
    Step::Years(2),
    Step::Years(5),
    Step::Years(10),
];

impl Step {
    /// Numéro de case temporelle à l'heure de la place
    fn bucket(self, t: DateTime<FixedOffset>) -> i64 {
        let local_seconds = t.timestamp() + i64::from(t.offset().local_minus_utc());
        let local_days = local_seconds.div_euclid(86_400);
        match self {
            Step::Minutes(m) => local_seconds.div_euclid(60 * m),
            Step::Hours(h) => local_seconds.div_euclid(3_600 * h),
            Step::Days(d) => local_days.div_euclid(d),
            // Le 01/01/1970 était un jeudi : +3 aligne les semaines sur le lundi
            Step::Weeks(w) => (local_days + 3).div_euclid(7 * w),
            Step::Months(m) => (i64::from(t.year()) * 12 + i64::from(t.month0())).div_euclid(m),
            Step::Years(y) => i64::from(t.year()).div_euclid(y),
        }
    }

    /// Format chrono des labels de ce pas
    fn format(self) -> &'static str {
        match self {
            Step::Minutes(_) | Step::Hours(_) => "%H:%M",
            Step::Days(_) | Step::Weeks(_) => "%d/%m",
            Step::Months(_) => "%b",
            Step::Years(_) => "%Y",
        }
    }

    /// Case plus large affichée en 2e ligne pour situer les labels fins
    fn context(self) -> (Step, &'static str) {
        match self {
            Step::Minutes(_) | Step::Hours(_) => (Step::Days(1), "%a %d/%m"),
            Step::Days(_) | Step::Weeks(_) => (Step::Months(1), "%b %Y"),
            // Les années n'ont pas de contexte plus large : on rappelle l'année de départ
            Step::Months(_) | Step::Years(_) => (Step::Years(1), "%Y"),
        }
    }
}

/// Un label posé à une colonne (relative au début du graphique)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub column: u16,
    pub text: String,
}

/// Les deux lignes de labels : graduations fines et contexte
#[derive(Debug, Default)]
pub struct TimeAxis {
    pub primary: Vec<Label>,
    pub secondary: Vec<Label>,
}

/// Indices des chandelles qui ouvrent une nouvelle case
fn boundaries(times: &[DateTime<FixedOffset>], step: Step) -> Vec<usize> {
    (1..times.len())
        .filter(|&i| step.bucket(times[i]) != step.bucket(times[i - 1]))
        .collect()
}

fn label_len(time: DateTime<FixedOffset>, format: &str) -> u16 {
    u16::try_from(time.format(format).to_string().chars().count()).unwrap_or(u16::MAX)
}

/// Le pas le plus fin dont les labels sont espacés d'au moins leur largeur + 1
fn choose_step(times: &[DateTime<FixedOffset>], columns: &[u16]) -> Option<Step> {
    LADDER.into_iter().find(|&step| {
        let indices = boundaries(times, step);
        !indices.is_empty()
            && indices
                .windows(2)
                .all(|w| columns[w[1]] - columns[w[0]] > label_len(times[w[0]], step.format()))
    })
}

/// Place les labels de gauche à droite, en sautant ceux qui chevauchent ou dépassent
fn place(
    times: &[DateTime<FixedOffset>],
    columns: &[u16],
    indices: &[usize],
    format: &str,
    width: u16,
) -> Vec<Label> {
    let mut labels = Vec::new();
    let mut next_free = 0;
    for &i in indices {
        let end = columns[i].saturating_add(label_len(times[i], format));
        if columns[i] >= next_free && end <= width {
            next_free = end + 1;
            labels.push(Label {
                column: columns[i],
                text: times[i].format(format).to_string(),
            });
        }
    }
    labels
}

/// Calcule les labels de l'axe du temps
///
/// `columns[i]` est la colonne de la chandelle `i` (croissantes, toutes < `width`).
pub fn time_axis(times: &[DateTime<FixedOffset>], columns: &[u16], width: u16) -> TimeAxis {
    if times.is_empty() {
        return TimeAxis::default();
    }
    // Pas trouvé (trop peu de chandelles) : pas de labels fins, seulement le contexte
    let step = choose_step(times, columns);
    let primary = step.map_or_else(Vec::new, |s| {
        place(times, columns, &boundaries(times, s), s.format(), width)
    });
    let (context_step, context_format) = step.map_or((Step::Days(1), "%a %d/%m"), Step::context);
    // La 1re chandelle porte toujours le contexte : on sait où commence le graphique
    let mut indices = vec![0];
    indices.extend(boundaries(times, context_step));
    let secondary = place(times, columns, &indices, context_format, width);
    TimeAxis { primary, secondary }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone, Timelike};

    fn series(
        offset: FixedOffset,
        start: DateTime<FixedOffset>,
        every: Duration,
        n: usize,
        market_hours: Option<(u32, u32)>,
    ) -> Vec<DateTime<FixedOffset>> {
        let mut out = Vec::new();
        let mut t = start;
        while out.len() < n {
            let open = market_hours.map_or(true, |(from, to)| {
                (from..to).contains(&t.hour()) && t.weekday().num_days_from_monday() < 5
            });
            if open {
                out.push(t.with_timezone(&offset));
            }
            t += every;
        }
        out
    }

    fn cols(n: usize, slot: u16) -> Vec<u16> {
        (0..u16::try_from(n).unwrap()).map(|i| i * slot).collect()
    }

    fn utc() -> FixedOffset {
        FixedOffset::east_opt(0).unwrap()
    }

    fn texts(labels: &[Label]) -> Vec<&str> {
        labels.iter().map(|l| l.text.as_str()).collect()
    }

    #[test]
    fn hourly_crypto_gets_date_labels() {
        // Bug revue : RegularDays comparait au chandelier précédent → 0 label en 24/7
        let start = utc().with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();
        let times = series(utc(), start, Duration::hours(1), 200, None);
        let axis = time_axis(&times, &cols(200, 1), 200);
        assert!(axis.primary.len() >= 5, "{:?}", texts(&axis.primary));
    }

    #[test]
    fn labels_never_overlap_nor_overflow() {
        let start = utc().with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();
        let times = series(utc(), start, Duration::minutes(5), 300, None);
        for (slot, width) in [(1, 300), (2, 600), (1, 40)] {
            let n = usize::from(width / slot).min(300);
            let axis = time_axis(&times[..n], &cols(n, slot), width);
            for row in [&axis.primary, &axis.secondary] {
                for pair in row.windows(2) {
                    let end = pair[0].column + u16::try_from(pair[0].text.chars().count()).unwrap();
                    assert!(end < pair[1].column, "{:?}", texts(row));
                }
                assert!(row
                    .iter()
                    .all(|l| l.column + u16::try_from(l.text.chars().count()).unwrap() <= width));
            }
        }
    }

    #[test]
    fn stock_intraday_uses_exchange_time() {
        // 30 min, 09:30–16:00 à New York : les labels doivent parler en heure de NY
        let ny = FixedOffset::west_opt(4 * 3600).unwrap();
        let start = ny.with_ymd_and_hms(2026, 9, 14, 9, 30, 0).unwrap();
        let times = series(ny, start, Duration::minutes(30), 130, Some((9, 16)));
        let axis = time_axis(&times, &cols(130, 2), 260);
        // 260 colonnes, 13 chandelles/jour à 2 colonnes : pas de 3 h → 09:30, 12:00, 15:00
        assert!(
            axis.primary
                .iter()
                .all(|l| ("09:00"..="16:00").contains(&l.text.as_str())),
            "{:?}",
            texts(&axis.primary)
        );
        assert!(
            axis.secondary
                .iter()
                .any(|l| l.text.starts_with("Tue 15/09")),
            "{:?}",
            texts(&axis.secondary)
        );
    }

    #[test]
    fn daily_candles_get_weeks_with_month_context() {
        // 1 colonne par jour : une semaine = 7 colonnes, assez pour "dd/mm"
        let start = utc().with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        let times = series(utc(), start, Duration::days(1), 500, None);
        let axis = time_axis(&times, &cols(500, 1), 500);
        assert!(
            axis.primary.iter().any(|l| l.text == "03/03"),
            "{:?}",
            texts(&axis.primary)
        );
        assert!(
            axis.secondary.iter().any(|l| l.text == "Jan 2026"),
            "{:?}",
            texts(&axis.secondary)
        );
    }

    #[test]
    fn weekly_candles_label_months_and_years() {
        let start = utc().with_ymd_and_hms(2016, 1, 4, 0, 0, 0).unwrap();
        let times = series(utc(), start, Duration::weeks(1), 520, None);
        let axis = time_axis(&times, &cols(520, 1), 520);
        assert!(
            axis.primary.iter().any(|l| l.text == "Mar"),
            "{:?}",
            texts(&axis.primary)
        );
        assert!(
            axis.secondary.iter().any(|l| l.text == "2020"),
            "{:?}",
            texts(&axis.secondary)
        );
    }

    #[test]
    fn empty_and_single_candle() {
        assert!(time_axis(&[], &[], 50).primary.is_empty());
        let one = [utc().with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap()];
        let axis = time_axis(&one, &[10], 50);
        assert!(axis.primary.is_empty() && axis.secondary.len() == 1); // contexte à gauche
    }
}
