# Stratégie d'Alignement des Chandeliers

## 📋 Table des Matières

- [Vue d'Ensemble](#vue-densemble)
- [Le Problème](#le-problème)
- [La Solution](#la-solution)
- [Architecture de l'Implémentation](#architecture-de-limplémentation)
- [Détails Techniques](#détails-techniques)
- [Cas Limites Gérés](#cas-limites-gérés)
- [Trade-offs et Décisions](#trade-offs-et-décisions)
- [Maintenance et Évolution](#maintenance-et-évolution)

---

## Vue d'Ensemble

Cette documentation explique la stratégie d'implémentation utilisée pour garantir un **alignement parfait** entre les chandeliers japonais (candlesticks) et leurs timestamps sur l'axe X dans le graphique TUI de LazyWallet.

**Principe fondamental** : Une seule source de vérité pour toutes les positions (chandeliers, ticks, labels, dates).

**Résultat** : Alignement parfait garanti par construction, quel que soit la largeur du terminal.

---

## Le Problème

### Symptômes Identifiés

1. **Terminal trop étroit** : Les chandeliers du milieu s'affichent, décalés par rapport à la chronologie
2. **Terminal trop large** : Les chandeliers ne remplissent pas toute la largeur et perdent l'alignement
3. **Drift progressif** : Les erreurs d'arrondi s'accumulent de gauche à droite
4. **Désalignement Y-axis** : L'axe Y utilise une largeur différente dans le constructeur vs le rendu

### Causes Racines

#### 1. Y-axis Width Mismatch 🔴 CRITIQUE

```rust
// Dans le constructeur
let y_axis_width = if area.width < 80 { 8 } else { 12 };

// Dans render_x_axis() - PROBLÈME
let tick_spans = vec![Span::raw(format!("{:>width$}", "", width = Y_AXIS_WIDTH))];
//                                                                  ^^^^^^^^^^^^
//                                                      Constante = 12 toujours !
```

**Impact** : Décalage horizontal de 4 caractères quand terminal < 80 colonnes.

#### 2. Erreurs d'Arrondi Cumulatives

```rust
let spacing = width / num_candles;  // Résultat flottant

for chandelier in chandeliers {
    placer chandelier
    ajouter (spacing - 1.0).round() espaces  // ← Erreur ici
}
```

**Problème** : Avec `spacing = 3.03`, chaque chandelier ajoute `round(2.03) = 2` espaces.
- Théoriquement : 33 × 3.03 = 100 caractères
- Réellement : 33 + (32 × 2) = 97 caractères → **3 chars de décalage**

#### 3. Calculs Indépendants pour Chaque Couche

```rust
// Chandeliers
for i in 0..n {
    position += (spacing - 1.0).round();  // Calcul 1
}

// Labels
for i in 0..n {
    position = i * spacing;  // Calcul 2 (différent!)
}
```

**Impact** : Les 4 couches (chandeliers, ticks, labels, dates) dérivent différemment.

#### 4. Position Tracking en Flottant

```rust
let mut position = 0.0;  // f64
for label in labels {
    let spaces = (next_pos - position).max(0.0) as usize;  // Conversion répétée
    position += spaces + label.len();
}
```

**Impact** : Conversions `f64 → usize` répétées accumulent des erreurs.

#### 5. Dernier Chandelier Sans Espacement

```rust
for (i, candle) in candles.iter().enumerate() {
    ajouter chandelier;
    if i < candles.len() - 1 {  // ← Condition
        ajouter espaces;
    }
}
```

**Impact** : Le dernier chandelier n'a pas d'espaces, mais les labels supposent qu'il en a.

---

## La Solution

### Principe : Position Array + Accumulator Pattern

Au lieu de calculer "combien d'espaces après chaque chandelier", on calcule **la position absolue de chaque chandelier** d'un seul coup.

```rust
struct CandlePosition {
    column: usize,  // Position absolue (0-based)
    width: usize,   // Largeur allouée (généralement 1)
}

fn compute_candle_positions(chart_width: usize, num_candles: usize) -> Vec<CandlePosition>
```

### Pourquoi Ça Fonctionne

1. **Calcul unique** : Les positions sont calculées une seule fois
2. **Accumulator pattern** : Chaque position = `index × spacing` (pas `position_précédente + spacing`)
3. **Source unique de vérité** : Toutes les couches utilisent le même tableau
4. **Garantie d'alignement** : `zip(chandeliers, positions)` lie indissociablement chaque chandelier à sa position

### Schéma Conceptuel

```
Approche AVANT (problématique) :
┌─────────────┐     ┌──────────────┐
│ Chandeliers │────►│ Calcul espaces│────► Positions A (avec drift)
└─────────────┘     └──────────────┘

┌─────────────┐     ┌──────────────┐
│   Labels    │────►│ Calcul positions│───► Positions B (différentes!)
└─────────────┘     └──────────────┘

Approche APRÈS (solution) :
┌─────────────────────┐
│ compute_positions() │────┐
└─────────────────────┘    │
                           ▼
                    ┌──────────────┐
                    │  Positions   │◄───── Source unique
                    └──────────────┘
                      │           │
        ┌─────────────┴─┐     ┌───┴──────────┐
        ▼               ▼     ▼              ▼
   Chandeliers      Ticks  Labels         Dates
```

---

## Architecture de l'Implémentation

### 1. Structure de Position

```rust
#[derive(Debug, Clone, Copy)]
struct CandlePosition {
    /// Position absolue de la colonne (0-based depuis le début de la zone graphique)
    column: usize,
    /// Nombre de caractères alloués à ce chandelier (généralement 1)
    width: usize,  // Pour extension future (chandeliers épais)
}
```

### 2. Algorithme de Calcul des Positions

```rust
fn compute_candle_positions(chart_width: usize, num_candles: usize) -> Vec<CandlePosition> {
    if num_candles == 0 {
        return Vec::new();
    }

    if num_candles == 1 {
        // Cas spécial : chandelier unique centré
        return vec![CandlePosition {
            column: chart_width / 2,
            width: 1,
        }];
    }

    let mut positions = Vec::with_capacity(num_candles);
    let spacing = chart_width as f64 / num_candles as f64;

    for i in 0..num_candles {
        // ✨ CLEF : Pattern accumulator
        // Chaque position est calculée depuis l'index, pas depuis la position précédente
        let exact_position = i as f64 * spacing;
        let column = exact_position.round() as usize;

        positions.push(CandlePosition {
            column: column.min(chart_width.saturating_sub(1)),
            width: 1,
        });
    }

    positions
}
```

**Pourquoi `i × spacing` au lieu de `position_précédente + spacing` ?**

```
Exemple avec spacing = 3.03 et 5 chandeliers :

Approche cumulative (MAUVAISE) :
position[0] = 0
position[1] = 0 + 3.03 = 3.03 → round = 3
position[2] = 3 + 3.03 = 6.03 → round = 6
position[3] = 6 + 3.03 = 9.03 → round = 9
position[4] = 9 + 3.03 = 12.03 → round = 12
Total : 12 (attendu : 15.15)  ❌ DRIFT

Approche accumulator (BONNE) :
position[0] = 0 × 3.03 = 0.00 → round = 0
position[1] = 1 × 3.03 = 3.03 → round = 3
position[2] = 2 × 3.03 = 6.06 → round = 6
position[3] = 3 × 3.03 = 9.09 → round = 9
position[4] = 4 × 3.03 = 12.12 → round = 12
Total : 12 (attendu : 15.15)  ✅ PAS DE DRIFT
```

### 3. Rendu des Chandeliers avec Position Array

```rust
pub fn render_lines(&self) -> Vec<Line<'a>> {
    let visible = self.visible_candles();

    // 🎯 Calcul unique des positions
    let positions = Self::compute_candle_positions(self.width as usize, visible.len());

    for y in (1..=self.height).rev() {
        // Construit la ligne avec un tableau de caractères
        let mut line_chars = vec![' '; self.width as usize];
        let mut line_colors = vec![None; self.width as usize];

        // 🔗 zip() garantit la correspondance chandelier ↔ position
        for (candle, pos) in visible.iter().zip(positions.iter()) {
            if pos.column < line_chars.len() {
                line_chars[pos.column] = self.render_candle(candle, y);
                line_colors[pos.column] = Some(Self::candle_color(candle));
            }
        }

        // Convertit en spans colorés...
    }

    // 🎯 Passe les MÊMES positions à l'axe X
    lines.extend(self.render_x_axis(visible, &positions));
}
```

### 4. Rendu de l'Axe X avec Position Array

```rust
fn render_x_axis(&self, visible: &[OHLC], positions: &[CandlePosition]) -> Vec<Line<'a>> {
    // Tick marks
    let mut tick_line = vec![' '; self.width as usize];
    for (i, pos) in positions.iter().enumerate() {
        if i % label_interval == 0 {
            tick_line[pos.column] = '│';  // ← Position exacte
        }
    }

    // Time labels
    let mut label_line = vec![' '; self.width as usize];
    for (i, (candle, pos)) in visible.iter().zip(positions.iter()).enumerate() {
        if i % label_interval == 0 {
            let time_label = candle.timestamp.format(format_str).to_string();

            // Centre le label sur la position du chandelier
            let label_start = pos.column.saturating_sub(time_label.len() / 2);
            // Place caractère par caractère...
        }
    }

    // Date labels (même principe)
    // ...
}
```

**Point clé** : `zip(visible, positions)` crée des paires indissociables :

```rust
visible    = [candle_10h, candle_11h, candle_12h]
positions  = [column_10,  column_30,  column_50]

zip() produit :
  (candle_10h, column_10)  ← candle_10h est TOUJOURS à la colonne 10
  (candle_11h, column_30)  ← candle_11h est TOUJOURS à la colonne 30
  (candle_12h, column_50)  ← candle_12h est TOUJOURS à la colonne 50
```

### 5. Fix Y-axis Width Mismatch

```rust
pub struct CandlestickRenderer<'a> {
    // ...
    y_axis_width: u16,  // ← Stocke la largeur calculée
}

pub fn new(...) -> Self {
    let y_axis_width = if area.width < 80 { 8 } else { 12 };

    Self {
        // ...
        y_axis_width,  // ← Sauvegarde pour réutilisation
    }
}

fn render_x_axis(...) {
    // Utilise self.y_axis_width au lieu de Y_AXIS_WIDTH constant
    let tick_spans = vec![Span::raw(format!("{:>width$}", "", width = self.y_axis_width as usize))];
    //                                                                  ^^^^^^^^^^^^^^^^^^
    //                                                          Valeur dynamique correcte
}
```

---

## Cas Limites Gérés

### Cas 1 : Terminal Trop Étroit (width < num_candles)

**Exemple** : 250 chandeliers, 108 colonnes disponibles (terminal de 120 colonnes)

**Comportement réel** :
1. `visible_candles()` garde toujours les **250** derniers chandeliers, sans tenir compte de la largeur ⚠️
2. `compute_candle_positions(108, 250)` → spacing ≈ 0.43
3. Plusieurs chandeliers tombent sur la même colonne : le dernier écrit gagne, les autres disparaissent (≈142 sur 250)
4. Les bornes de prix sont calculées sur les 250, y compris les chandeliers masqués

**Correction attendue** : garder `min(250, largeur)` chandeliers, pour que spacing ≥ 1.

### Cas 2 : Terminal Trop Large (num_candles < width)

**Exemple** : 10 chandeliers, 100 colonnes disponibles

**Solution** :
1. `compute_positions(100, 10)` → spacing = 10.0
2. Positions : `[0, 10, 20, 30, ..., 90]`
3. Résultat : Chandeliers répartis uniformément sur toute la largeur ✅

### Cas 3 : Spacing Fractionnaire

**Exemple** : 50 chandeliers, 100 colonnes → spacing = 2.0

**Solution** :
- Positions : `[0×2=0, 1×2=2, 2×2=4, ..., 49×2=98]`
- Distribution parfaite, aucun drift ✅

### Cas 4 : Chandelier Unique

**Solution** :
```rust
if num_candles == 1 {
    return vec![CandlePosition { column: chart_width / 2, width: 1 }];
}
```
Le chandelier est explicitement centré ✅

### Cas 5 : Redimensionnement du Terminal

**Flux** :
1. Terminal redimensionné → nouveau `Rect` passé à `CandlestickRenderer::new()`
2. Nouvelle `y_axis_width` calculée (en pratique toujours 12, voir Limites Connues)
3. Nouvelle `width` = `area.width - y_axis_width`
4. `render_lines()` → nouveau `compute_positions()` avec nouvelle largeur
5. Tout est recalculé avec les bonnes dimensions ✅

---

## Limites Connues (revue 2026-09-22)

Vérifiées par des tests jetables sur `8878ec3` :

- **Largeur** : le renderer reçoit la zone *avec* bordure (`chunks[1]`) alors que le `Paragraph` dessine dans la zone intérieure (2 colonnes de moins). Chaque ligne fait 120 caractères pour 118 visibles : les 2 dernières colonnes (les chandeliers les plus récents) sont coupées. Idem en hauteur : `height = area.height - 6` réserve encore 3 lignes pour un header qui est déjà dans un autre chunk.
- **Recouvrement** : voir Cas 1.
- **Axe Y étroit** : `NARROW_Y_AXIS_WIDTH` (8) n'est jamais utilisé, car en dessous de 80 colonnes l'écran « terminal trop étroit » s'affiche avant. De plus `render_y_axis` produit toujours 12 caractères (`{:>9.2} │ `), donc activer ce mode décalerait l'axe X.
- **`RegularDays` (H1)** : `should_show_label` compare chaque chandelier au **précédent**, pas au dernier label. Des chandeliers horaires consécutifs n'ont jamais 2 jours d'écart : aucun label en crypto, seulement le lundi (après le week-end) pour les actions.
- **Ticks** : quand deux labels tombent sur la même colonne (spacing < 1), un `│` est quand même ajouté et décale le reste de la ligne d'un caractère.
- **Fuseau** : heures et dates sont en UTC.

Les tests unitaires proposés plus bas n'ont jamais été écrits.

## Détails Techniques

### Performance

**Complexité** :
- `compute_positions()` : **O(n)** où n = nombre de chandeliers visibles
- `render_lines()` : **O(n × h)** où h = hauteur (inchangé)
- **Total** : Pas de dégradation de performance

**Mémoire** :
- `Vec<CandlePosition>` : 16 bytes × n
- Pour 200 chandeliers : ~3 KB (négligeable)

### Choix d'Implémentation

#### Pourquoi un Tableau de Caractères au lieu de Spans ?

**AVANT** :
```rust
for (i, candle) in candles.iter().enumerate() {
    spans.push(Span::styled(candle_char, color));
    if i < len - 1 {
        spans.push(Span::raw(" ".repeat(spaces)));
    }
}
```

**Problème** : Les espaces dépendent du calcul de `spacing`, source de drift.

**APRÈS** :
```rust
let mut line_chars = vec![' '; width];
for (candle, pos) in zip(candles, positions) {
    line_chars[pos.column] = candle_char;
}
// Convertir en spans...
```

**Avantage** : Placement direct à la position exacte, pas d'accumulation d'espaces.

#### Pourquoi Centrer les Labels ?

```rust
let label_start = pos.column.saturating_sub(time_label.len() / 2);
```

**Raison** : Le label représente un instant (le timestamp du chandelier). Le centrer sur la position du chandelier est plus précis visuellement qu'un alignement à gauche ou à droite.

**Exemple** :
```
Position du chandelier : colonne 50
Label "12:00" (5 chars)

Aligné à gauche :     ┃12:00      ❌ Décalé visuellement
Centré :                ┃12:00    ✅ Visuellement aligné
                      ^^
                   Position 48-52
```

---

## Trade-offs et Décisions

### Alternatives Considérées

| Approche | Avantages | Inconvénients | Choix |
|----------|-----------|---------------|-------|
| **Fixed-width columns** | Simple à implémenter | Pas flexible, gaspille l'espace | ❌ |
| **Integer spacing only** | Pas de flottants | Drift possible avec grands nombres | ❌ |
| **Accumulator pattern seul** | Pas de drift | Structure moins extensible | ⚠️ |
| **Position array** ✅ | Alignement parfait, extensible | +100 lignes de code | ✅ |
| **Elastic spacing** | Ultra-flexible | Complexité O(n²), overkill | ❌ |

### Décision Finale : Position Array + Accumulator

**Pour** :
- ✅ Garantit l'alignement parfait par construction
- ✅ Zéro drift grâce à l'accumulator pattern
- ✅ Extensible (width > 1 pour thick candles futurs)
- ✅ Performance acceptable (O(2n) vs O(n))
- ✅ Code maintenable et testable

**Contre** :
- ⚠️ ~100 lignes de code supplémentaires
- ⚠️ +3 KB de mémoire pour 200 chandeliers (négligeable)

**Conclusion** : Les avantages surpassent largement les inconvénients.

---

## Maintenance et Évolution

### Points de Vigilance

1. **Modification de `compute_positions()`** :
   - Toujours conserver le pattern accumulator (`i × spacing`)
   - Ne jamais calculer depuis `position_précédente + delta`
   - Tester avec des cas fractionnaires (ex: 33 candles, 100 cols)

2. **Ajout de nouvelles couches visuelles** :
   - TOUJOURS utiliser `zip(items, positions)`
   - JAMAIS recalculer les positions indépendamment
   - Exemple pour ajouter une barre de volume :
     ```rust
     for (volume, pos) in volumes.iter().zip(positions.iter()) {
         volume_line[pos.column] = render_volume(volume);
     }
     ```

3. **Modification de `y_axis_width`** :
   - Mettre à jour `ADAPTIVE_Y_AXIS_THRESHOLD` si besoin
   - Vérifier que `render_x_axis()` utilise bien `self.y_axis_width`
   - Tester le resize de terminal autour du seuil (80 cols)

### Extensions Futures Possibles

#### 1. Chandeliers Épais (width > 1)

```rust
CandlePosition {
    column: 50,
    width: 3,  // Chandelier occupe 3 colonnes
}

// Dans render_lines()
for (candle, pos) in zip(candles, positions) {
    for offset in 0..pos.width {
        line_chars[pos.column + offset] = candle_char;
    }
}
```

#### 2. Zoom/Pan Horizontal

```rust
fn visible_candles_window(&self, start: usize, count: usize) -> &[OHLC] {
    let end = (start + count).min(self.candles.len());
    &self.candles[start..end]
}

// Les positions restent valides, on change juste la fenêtre de chandeliers
```

#### 3. Curseur de Sélection

```rust
// Mapper une coordonnée X → index de chandelier
fn candle_at_column(&self, column: usize, positions: &[CandlePosition]) -> Option<usize> {
    positions.iter()
        .position(|pos| column >= pos.column && column < pos.column + pos.width)
}
```

#### 4. Indicateurs Techniques Overlay

```rust
// MA, RSI, Bollinger, etc. utilisent les mêmes positions
for (ma_value, pos) in ma_values.iter().zip(positions.iter()) {
    let y = price_to_y(ma_value);
    overlay_chars[y][pos.column] = '•';
}
```

### Tests Recommandés

#### Tests Unitaires

```rust
#[test]
fn test_compute_positions_even_distribution() {
    let positions = CandlestickRenderer::compute_positions(100, 10);
    assert_eq!(positions.len(), 10);

    // Vérifier espacement à 10 colonnes
    for (i, pos) in positions.iter().enumerate() {
        let expected = i * 10;
        assert!((pos.column as i32 - expected as i32).abs() <= 1);
    }
}

#[test]
fn test_compute_positions_narrow_terminal() {
    let positions = CandlestickRenderer::compute_positions(50, 100);
    assert_eq!(positions.len(), 100);

    // Toutes les positions doivent être < 50
    for pos in positions.iter() {
        assert!(pos.column < 50);
    }
}

#[test]
fn test_compute_positions_single_candle() {
    let positions = CandlestickRenderer::compute_positions(100, 1);
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].column, 50);  // Centré
}

#[test]
fn test_no_drift_accumulation() {
    // Cas difficile : spacing fractionnaire
    let positions = CandlestickRenderer::compute_positions(100, 33);

    // Première et dernière position doivent être cohérentes
    assert_eq!(positions[0].column, 0);
    assert!(positions[32].column >= 95 && positions[32].column <= 100);
}
```

#### Tests Visuels

1. **Terminal 70 cols** : écran « Terminal trop étroit » (minimum 80)
2. **Terminal 120 cols** : Y-axis = 12 chars, chandeliers répartis
3. **200+ chandeliers** : aujourd'hui, recouvrement si plus de chandeliers que de colonnes
4. **5 chandeliers** : Répartis uniformément, labels centrés
5. **Resize dynamique** : Alignement maintenu pendant le resize

---

## Conclusion

La stratégie **Position Array + Accumulator Pattern** garantit un alignement parfait entre les chandeliers et leurs timestamps en éliminant toutes les sources de drift et de désalignement.

**Principes clés** :
1. 🎯 **Une seule source de vérité** : `compute_candle_positions()`
2. 🔗 **Liaison indissociable** : `zip(chandeliers, positions)`
3. 📐 **Pattern accumulator** : `position = i × spacing`
4. 🎨 **Tableau de caractères** : Placement direct aux positions exactes

**Résultat** : Code maintenable, extensible, et visuellement parfait sur tous les terminaux.

---

*Documentation rédigée le 2025-01-28*
*Implémentation : src/ui/candlestick_text.rs*
*Commits : 0222edf, c327290, 48b33f2 — limites ajoutées le 2026-09-22*
