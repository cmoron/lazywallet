# Rendu du graphique en chandeliers

> Code : `src/ui/chart/`. Remplace `candlestick-alignment.md` (voir l'historique git).

## Principe

Le graphique est un `Widget` ratatui (`CandleChart`) qui écrit directement dans
les cellules du `Buffer`, **uniquement dans le `Rect` qu'il reçoit**. L'écran
(`chart/mod.rs`) lui passe la zone *intérieure* du cadre (`block.inner(area)`) :
aucune ligne ne peut déborder sur la bordure.

```
┌ 30m ──────────────────────────────────┐   ← cadre (chart/mod.rs)
│        ╷╷           ╷ ╷╽┃┃┃┃┃╵ │       │
│┈┈┈┈┈┈┈╻╽┃┈┈┈┈┈┈┈┈┈┈┈╽╽╽┃╵┈┈┈┈┈┈┤ 339.75│   ← repère du dernier prix
│ ╻╻╽╻╻╻╹ ┃        ╷╽╽╷┃╵         ┤ 335   │   ← graduation ronde
│─────────┬────────────┬──────────┘       │   ← trait + ticks (┬)
│         18/09        21/09              │   ← labels fins
│Sep 2026                                  │   ← contexte
└───────────────────────────────────────────┘
  plot (chandelles)                 axe des prix
```

`AXIS_ROWS = 3` lignes sont réservées sous le graphique ; tout le reste de la
hauteur est pour les chandelles. En dessous de `MIN_WIDTH = 30` colonnes ou
`MIN_HEIGHT = 8` lignes, le widget affiche « Terminal trop petit ».

## Densité et colonnes (`geometry.rs`)

Choix : **plus le terminal est large, plus on remonte dans le temps.** On
n'étire jamais les chandelles.

- `slot_width` : 1 colonne par chandelle ; 2 (chandelle + espace) dès que le
  graphique fait au moins 120 colonnes (60 chandelles espacées).
- `visible_columns(plot_width, total)` garde les `plot_width / slot` chandelles
  les plus récentes et les aligne **à droite**, contre l'axe des prix. Avec peu
  de données, l'espace libre reste à gauche.
- Chaque chandelle a une colonne entière, calculée une seule fois et partagée
  par toutes les couches (chandelles, ticks, labels) : alignement garanti,
  jamais deux chandelles sur la même colonne.

L'historique demandé à Yahoo (`Interval::history_days`) est dimensionné pour
remplir un terminal large (~500 chandelles pour une action).

## Axe des prix (`price_axis.rs`)

- `nice_step` choisit un pas rond (1, 2, 5 × 10^k), environ une graduation toutes
  les `ROWS_PER_TICK = 4` lignes.
- `decimals_for(step)` donne la précision des labels (0 pour un pas de 5, 3 pour
  un pas de 0.002 sur l'EUR/USD). Test par multiplication plutôt que `log10`,
  à cause des flottants.
- Le dernier prix est affiché en inversé dans la couleur de la chandelle, avec la
  précision du ticker (`priceHint` Yahoo) si elle est plus fine que celle des
  graduations, et une ligne `┈` dans les cases vides de sa ligne.
- La largeur de l'axe dépend des labels, qui dépendent des chandelles visibles,
  qui dépendent de la largeur : deux passes (largeur provisoire de 10, puis
  réelle). Plafond connu : si la 2e passe élargit la plage d'un chiffre, le
  dernier caractère d'un label est coupé.

## Axe du temps (`time_axis.rs`)

Pas de seuils de largeur écrits à la main :

1. Échelle de pas candidats, du plus fin au plus grossier : 5/15/30 min,
   1/2/3/6/12 h, 1/2 jours, 1/2 semaines, 1/3/6 mois, 1/2/5/10 ans.
2. Pour un pas, une chandelle reçoit un label quand sa **case** (`bucket`)
   diffère de celle de la chandelle précédente. Les trous du marché (nuits,
   week-ends) sont donc gérés d'office.
3. On garde le premier pas dont les labels sont espacés d'au moins leur largeur
   + 1 colonne.
4. Une 2e ligne donne le contexte (jour sous les heures, mois sous les jours,
   année sous les mois), dont toujours celui de la première chandelle.
5. Placement de gauche à droite : un label qui chevauche le précédent ou dépasse
   à droite est sauté.

Les heures sont celles de la **place de cotation** : `OHLCData::local_time`
applique `utc_offset` (`meta.gmtoffset` de Yahoo). Plafond connu : décalage fixe
pris au moment du chargement, donc un changement d'heure dans la fenêtre décale
d'1 h les labels intraday les plus anciens (évolution : `chrono-tz` avec
`exchangeTimezoneName`).

## Dessin des chandelles (`widget.rs`)

`glyph(candle, y, scale)` vient de
[cli-candlestick-chart](https://github.com/Julien-R44/cli-candlestick-chart) :
trois zones (mèche haute, corps, mèche basse) et des seuils 0.25 / 0.75 pour une
précision d'un demi-caractère. Les mèches restent fines (`│ ╷ ╵`). Le corps a
deux jeux de caractères selon la densité :

- chandelles espacées (2 colonnes chacune) : blocs pleins `█ ▄ ▀`, un corps
  lisible qui remplit la cellule ;
- chandelles serrées (1 colonne) : traits `┃ ╻ ╹ ╽ ╿`, car des blocs voisins
  formeraient un mur continu. `Scale` convertit un prix en
hauteur fractionnaire ; une série plate est élargie de ±1 % pour éviter une
division par zéro.

## Tests

- `geometry.rs`, `price_axis.rs`, `time_axis.rs` : fonctions pures.
- `widget.rs` : rendu dans un `Buffer` nu, puis lecture des cellules (largeur
  exacte, dernière chandelle contre l'axe, petites tailles, prix plat).
- `chart/mod.rs` : écran complet via `TestBackend`.
