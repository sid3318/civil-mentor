# Civil Mentor: design plan

Status: D0 (foundations), D1 (shell) and D2 (every screen) are built and tested. D3 (polish) is partly done; see its list. Written and built 17 Nov 2026. The visual reference is `design/mockup.html` (open it in a browser). It shows Today, Notes, the exam room in dark mode, and Analysis.

PLAN.md says what the app does. This file says how it looks and how its screens are laid out. Every phase in PLAN.md is built, so this plan replaces the look of what already exists. It adds no features.

## 1. What is wrong with the current look

Screenshots of every screen are in `target/shots/` after `cargo test`.

1. **Everything sits on one flat plane.** The sidebar, the title bar and the content share one beige, split only by 1 px lines, so nothing stands out as the place to work.
2. **The type shouts.** Body text is 15 to 16 px, headings are 32 px bold and card titles are bold, so nothing is quieter than anything else. Today shows five tasks and half a question in a full window.
3. **Every card is the same bordered box.** The plan, the mentor and the week look equally important.
4. **The controls look like defaults.** There are tiny outline buttons of different heights, a navy primary, a mustard "marked" colour and stock blue charts. None of it comes from one palette.
5. **Numbers are sentences.** "0 of 5 done · 67 min" and "99% · 1 min left" are text where a ring or a bar would read at a glance.
6. **The title bar wastes a row.** It holds the app name, a centred search box and the countdown as plain text.
7. **Long titles wrap to three lines** in the notes and story lists, because the panes are narrow for the font size.
8. **Dark mode is the library default.** It is pure black with grey boxes, not a designed theme.

## 2. Direction

### Principles

1. **One surface to work on.** The window background and the sidebar fall back. The work sits on one raised panel, the **canvas**, inset 8 px with 12 px corners. Inside the canvas everything is flat. Shadows appear only where something floats: popovers, dialogs and the palette.
2. **Quiet chrome, colour with meaning.** The interface is warm graphite. Colour appears only where it means something:
   - one accent for action, selection and progress;
   - one colour per subject, the same everywhere;
   - green for right, red for wrong, amber for marked.
3. **Dense, not cramped.** UI text is 13 px with 30 px rows on a 4 px grid. Whitespace comes from alignment, not from padding everything.
4. **Data has a shape.** Every number a student watches has a visual form: rings for today's progress, bars for accuracy, heat cells for study time, chips for status.
5. **Reading is its own mode.** Notes and stories use a serif text face at 17 px in a 680 px column. The rest of the window gets out of the way.
6. **The keyboard is visible.** Buttons that have a shortcut show it, for example "Submit ⌘↵", "Mark M" or "Next →".
7. **Dark mode is designed.** It uses layered graphite (window, canvas, card, popover) with brighter accents. It is not an inversion of light mode.

### What we take from Capy, and what is ours

| From Capy | Ours |
|---|---|
| Inset canvas: the sidebar on the window background, the work in a rounded panel | Warm graphite neutrals instead of Capy's cool violet-grey |
| Hairline borders as alpha of the text colour, not grey hex values | **Peacock** accent (`#006D99`), not Capy's violet or teal |
| 13 to 14 px UI type in Inter, medium weights, tight headings | A serif reading face (Source Serif 4) for notes and stories |
| Header strip inside the canvas: breadcrumb, segmented tabs, actions | A colour per subject, so a subject reads the same on every screen |
| Right-hand inspector pane for context | Study visuals: day ring, Leitner boxes, question palette, heat strip |
| Segmented controls, count badges in the sidebar, keys inside buttons | Dot-field texture on the Today greeting and empty states, tinted with the accent |
| Settings as grouped cards of rows: title and help on the left, control on the right | Focus mode for the exam room, the Mains editor and review sessions |

We take Capy's structure and restraint. We do not take its brand, illustrations or wording.

## 3. Tokens

Every colour, size and radius in screen code comes from these tokens, through the theme or `src/ui.rs`. Screen files never use a raw hex value.

### Colour

Light is **Paper** (the existing theme name, kept because tests assert it) and dark is **Ink**. The neutrals are warm, with a hue of 85 at very low chroma. The values were picked in OKLCH, and the text colours pass WCAG AA on the canvas.

| Role | Paper | Ink | Used for |
|---|---|---|---|
| `chrome` | `#F2F0ED` | `#0D0C0A` | Window background, behind the sidebar and canvas |
| `canvas` | `#FCFBFA` | `#151412` | The main panel |
| `card` | `#FFFFFF` | `#1D1C1A` | Cards, inputs, the selected segment, the search box |
| `popover` | `#FFFFFF` | `#242321` | Menus, the palette, dialogs |
| `sunken` | `#F5F4F2` | `#191816` | Tracks, segmented backgrounds, table heads, code |
| `hover` | text at 4.5% (`#1E1D1A0B`) | white at 4% (`#FFFFFF0A`) | Row and button hover |
| `selected` | text at 7% (`#1E1D1A12`) | white at 7% (`#FFFFFF12`) | Active sidebar row, pressed controls |
| `border-1` | text at 8% (`#1E1D1A14`) | white at 8% (`#FFFFFF14`) | Hairlines, card edges, the canvas edge |
| `border-2` | text at 12% (`#1E1D1A1F`) | white at 12% (`#FFFFFF1F`) | Buttons, inputs |
| `border-3` | text at 16% (`#1E1D1A29`) | white at 16% (`#FFFFFF29`) | Strong dividers, hovered inputs |
| `text` | `#1E1D1A` (16.3:1) | `#ECEBE8` (15.4:1) | Primary text |
| `text-2` | `#5C5B57` (6.6:1) | `#B2B1AE` (8.6:1) | Secondary text, sidebar labels |
| `text-3` | `#6D6C69` (5.1:1) | `#8A8986` (5.3:1) | Meta lines, captions, counts |
| `text-4` | `#A6A4A2` | `#5F5D5B` | Placeholders, disabled, not visited |
| `accent` | `#006D99` | `#42ACD2` | Primary buttons, selection, focus ring, progress |
| `accent-hover` | `#00618C` | `#54BCE2` | Primary button hover |
| `accent-soft` | accent at 10% (`#006D991A`) | accent at 14% (`#42ACD224`) | Selected list row, soft buttons, info callouts |
| `on-accent` | `#FFFFFF` (5.8:1) | `#0D0C0A` (7.5:1) | Text on accent |
| `attention` | `#EF9D25` | `#F2A635` | Marked for review, streak flame, the trap. Used as a fill or icon, never as small text |
| `highlight` | `#F5CB708C` | `#96680059` | The student's highlights in the reader |
| `success` | `#2A904B` | `#4CB86A` | Right answers, done, gains in marks |
| `danger` | `#C83A35` | `#E8605B` | Wrong answers, losses in marks, destructive actions |

The accent is one token. If Peacock is not right, change it here and in `assets/theme.json` and nothing else moves.

### Subject colours

Each subject in `mentor::SUBJECTS` keeps one colour on every screen. The colour shows as a dot in lists, a chip on a note, a series in a chart, a bar in the mistake book and a tint on a drill card. The hues are spaced about 40° apart at matched lightness and kept clear of the accent's hue. Mains topics take the colour of the nearest Prelims subject, so the mapping lives in one function, `ui::subject_color(topic)`.

| Subject | Covers | Paper | Ink |
|---|---|---|---|
| History and culture | `pre.gs.ancient`, `medieval`, `modern`, `culture`; Mains GS-1 except geography | `#B66C18` | `#D38C49` |
| Geography | the three Prelims geography subjects; `mains.gs1.geography` | `#00988F` | `#0FB6AC` |
| Polity | `pre.gs.polity`; Mains GS-2; `mains.gs3.security` | `#9669BB` | `#B389D8` |
| Economy | `pre.gs.economy`; `mains.gs3.economy`, `agriculture` | `#6679CD` | `#8499E9` |
| Environment | `pre.gs.environment`; `mains.gs3.environment`, `disaster` | `#409551` | `#67B373` |
| Science and technology | `pre.gs.general-science`, `science-tech`; `mains.gs3.science-tech` | `#B85C8B` | `#D67EA9` |
| Current affairs | `pre.gs.current` | `#C25E53` | `#DF8074` |
| Ethics (colour only) | Mains GS-4 | `#8C8300` | `#AAA23F` |
| Other (colour only) | CSAT, Essay, Papers A and B | `#6D7C8C` | `#8B9AAB` |

The last two rows add colours, not subjects: `mentor::SUBJECTS` and the stored answers stay as they are.

### Type

| Font | Use | Notes |
|---|---|---|
| **Inter** (400, 500, 600) | All UI text | Embedded, SIL Open Font License. Static files, not the variable font, so weight matching is predictable in GPUI |
| **Inter Display** (600) | Headings of 20 px and up | Tighter at large sizes. GPUI has no letter-spacing control, so this is how headings get tight |
| **Source Serif 4** (400, italic, 600) | Note and story body text, the Mains editor | Embedded, SIL OFL. A Serif or Sans switch in the reader's Aa menu, saved as a setting |
| System mono | Key hints, the rare code block | Not embedded |

| Style | Size / line height | Weight | Used for |
|---|---|---|---|
| `display` | 28 / 34 | Inter Display 600 | The Today greeting, a test's score |
| `title-1` | 26 / 32 | Inter Display 600 | Note and story titles |
| `title-2` | 18 / 24 | Inter Display 600 | Headings inside notes, page sections |
| `title-3` | 14 / 20 | 600 | Card titles, the canvas header title |
| `body` | 14 / 21 | 400 | Text inside cards, settings help |
| `ui` | 13 / 18 | 400 or 500 | Default: sidebar, rows, buttons, tabs |
| `meta` | 12 / 16 | 400 | The second line of a row, chips, table heads |
| `caption` | 11 / 14 | 500 | Group labels, counts, chart axes, key hints |
| `reader` | 17 / 28 (13 to 24 adjustable) | Source Serif 4 400 | Note and story body |
| `numeric` | any | 500 or 600, tabular figures | Timers, scores, counts, table columns |

Tabular figures stop timers and score columns from jumping. Use Inter's `tnum` feature where GPUI's font features allow it. Otherwise give the number a fixed-width box.

### Space, radius, elevation, motion

- **Spacing** is a 4 px grid: 4, 6, 8, 12, 16, 20, 24, 32, 48. Canvas content has 32 px side padding, or 24 px below 1100 px of canvas width. Cards have 16 px padding, or 12 px when dense.
- **Radius** is 4 for chips and key hints, 6 for buttons, inputs and rows, 8 for cards, 12 for the canvas, dialogs and the palette, and full for pills, rings and avatars. `theme.radius = 6`, `theme.radius_lg = 12`.
- **Elevation** has four levels:
  - e0, flat: inside the canvas.
  - e1, cards: a `border-1` edge plus `0 1px 2px` black at 4% in Paper. Ink uses the border only.
  - e2, the canvas: a `border-1` edge plus `0 1px 3px` black at 5%.
  - e3, popover, palette and dialog: a `border-2` edge plus `0 12px 32px` black at 14% and `0 2px 6px` black at 8%. Ink uses 45% and 30%.
- **Motion** is 120 ms ease-out for hover and press, and 180 ms for panels opening and closing. Nothing moves when reduce-motion is on, which the screenshot test already sets.
- **Icons** are Lucide, which is already in use. They are 16 px in the sidebar and buttons, 14 px in dense rows, `text-3` when idle and `text` when active.

## 4. Layout structure

### Window anatomy

```
 chrome background
┌───────────────────────────────────────────────────────────────────────────────┐
│ ● ● ●        ◧  ‹  ›  │ ┌──────────────────────────────────────────────────┐  │
│                       │ │ ◉ Title or breadcrumb  [ Tab | Tab ]   actions ◨ │  │ header 40
│ ⌕ Search          ⌘K  │ ├──────────────────────────────────┬───────────────┤  │
│                       │ │                                  │               │  │
│ STUDY                 │ │                                  │   Inspector   │  │
│ ☀ Today          2/5  │ │          Content                 │   300 px      │  │
│ ▯ Notes               │ │          (one per screen)        │   (optional,  │  │
│ ✎ Practice            │ │                                  │   toggled     │  │
│ ▤ Current affairs   3 │ │                                  │   with ◨)     │  │
│ ▥ Analysis            │ │                                  │               │  │
│                       │ │                                  │               │  │
│ LIBRARY               │ │                                  │               │  │
│ ↻ Revision       (12) │ │                                  │               │  │
│ ✍ Mains               │ │                                  │               │  │
│ ☰ Syllabus       34%  │ │                                  │               │  │
│ ▤ Notebook            │ │                                  │               │  │
│                       │ │                                  │               │  │
│ ┌ 🔥 10-day streak ─┐ │ │                                  │               │  │
│ │ 187 days ▬▬▬░░░░ │ │ │                                  │               │  │
│ └───────────────────┘ │ │                                  │               │  │
│ ⏱ Focus        25:00  │ │                                  │               │  │
│ ⚙ Settings            │ └──────────────────────────────────┴───────────────┘  │
└───────────────────────────────────────────────────────────────────────────────┘
  sidebar 232 px          canvas: inset 8 px top, right and bottom; 12 px corners
```

- **No separate title bar.** The macOS traffic lights sit in the sidebar's top row, at about (18, 21). The sidebar toggle, Back and Forward sit at the right end of that row. This frees the 52 px row the title bar takes today.
- **Drag regions.** The sidebar's top row and the canvas header move the window, the way gpui-kit's `TitleBar` does (`window_control_area(Drag)` and `start_window_move`).
- **Alignment.** The sidebar's top row is 56 px and the canvas header is 40 px below an 8 px inset, so both centre at y = 28.
- **Default window** stays 1280 × 820, and the minimum stays 900 × 600.

### Sidebar (232 px)

- **Top row (56 px):** traffic lights, then ◧ (fold the sidebar, ⌘\), ‹ and › (⌘[ and ⌘]).
- **Search (30 px):** a field-like button on `card` that reads "Search" and shows ⌘K. It opens the palette.
- **Two groups:** "Study" (Today, Notes, Practice, Current affairs, Analysis) and "Library" (Revision, Mains, Syllabus, Notebook). Group labels are `caption` in `text-3`. Rows are 30 px with a 16 px icon and 13 px label. The active row gets a `selected` background with `text` colour and medium weight.
- **Counts, through the menu item's `suffix`:**
  - Today shows plan progress ("2/5").
  - Current affairs shows unread stories in the latest brief.
  - Syllabus shows coverage ("34%").
  - Revision shows items due as an accent pill, and only when there are any. It is the one count that asks for action.
- **Footer:**
  - A streak card on `card` with the flame in `attention`, "10-day streak", "187 days to Prelims", and a thin bar from the first day of use to the exam date.
  - The focus timer row: "Focus 25:00" with a play button. It shows the time left while running.
  - Settings.
- **Rail (72 px):** icons only, with tooltips and count dots. The width fits the traffic lights. It folds on its own for Notes, and unfolds afterwards unless the student folded it themselves. That rule exists today for Notes and extends to Current affairs.

### Canvas and header

- The canvas is `canvas` with a 12 px radius, a `border-1` edge and e2 elevation, inset 8 px from the top, right and bottom of the window.
- **Header (40 px), split by a `border-1` line, with three slots:**
  - **Left:** the screen's icon and title (`title-3`), or a breadcrumb for something inside a section ("Notes › GS-2 · Polity › Fundamental Rights", with ancestors in `text-3`).
  - **Centre-left:** the screen's segmented control or tabs, such as Read | Distilled, Full | Rapid, or the Analysis tabs.
  - **Right:** up to three actions at 28 px, at most one of them primary, then ◨ to toggle the inspector on screens that have one.
- **Content** scrolls inside the canvas, with one scroll position per screen (as now). Dashboards centre at up to 1120 px wide on a 12-column grid with 16 px gutters. Readers centre at 680 px, or 920 px when wide.

### Panes and inspector

- **List and reader screens** (Notes, Current affairs, Notebook) put the panes inside the canvas: a list (272 px, `border-1` on its right), the reader, then the inspector.
- **Inspector (300 px)** sits on the right with `border-1` on its left and its own scroll. It has a 40 px tab row when it has more than one view. It holds context, never the main task:
  - Notes: outline, linked questions, highlights.
  - Current affairs: story facts and linked notes.
  - Mains: the model answer, once it is allowed.
- **Lists** use one line per title, cut with an ellipsis, plus one meta line. No title wraps to three lines.

### Width rules

| Canvas width | Behaviour |
|---|---|
| 1200 px and up | Sidebar and inspector both open |
| 900 to 1199 px | The inspector closes and opens as an overlay sheet from ◨. Dashboards drop from 3 columns to 2 |
| Window at 900 px wide (the minimum) | List and reader screens fold the sidebar to the rail. The list pane narrows to 240 px |

### Focus mode

The exam room, a Mains answer in progress and a review session take the whole window, as PLAN.md requires for the exam room. They have no sidebar and the canvas is inset 8 px on all sides. The header is 52 px with the task's title, a centred timer pill and the exit and submit actions. Leaving needs a confirmation if work would be lost, as it does now.

## 5. The component kit (`src/ui.rs`)

Shared visuals live in one file of plain functions, in the style of today's `screens::page()` helpers. They are not builders or traits. Each part in the table is used by at least one screen, and anything not listed stays in its screen file until a second screen needs it.

| Part | Look | Built from |
|---|---|---|
| `tokens(cx)` | The colours in section 3, picked by theme mode | Two `const` tables |
| `subject_color(topic)` | The subject colour for a topic | `mentor::subject_of_topic`, with the Mains fallback |
| `header(...)` | The 40 px canvas header with left, centre and right slots | `div` |
| `card(...)` | `card` background, 8 px radius, e1 elevation, optional title row with a right-hand action | `div` |
| `section_label(...)` | `caption` in `text-3` with optional count and action | `div` |
| `row(...)` | 30 px or 44 px list row: leading icon or dot, title, meta, trailing slot. Uses hover and selected states | `div` |
| `chip(...)` | 20 px pill. Neutral (`sunken`), subject (12% tint and subject text), or status | `div` |
| `dot(color)` | 8 px subject dot | `div` |
| `progress(value, color)` | 4 px bar on a `selected` track | `div` |
| `ring(value, size, color)` | Donut from 32 to 96 px with the value in the middle | gpui-kit `PieChart` with an inner radius |
| `stat(label, value, delta)` | Number tile: `caption` label, 22 px tabular value, delta in success or danger | `div` |
| `callout(kind, ...)` | Tinted block with a 3 px bar: trap (attention), info (accent), right or wrong | `div` |
| `key_button(...)` | Button with a key hint inside it ("Submit ⌘↵") | gpui-kit `Button` plus `Kbd` |
| `segmented(...)` | Header tabs and mode switches | gpui-kit `TabBar` segmented, or `ButtonGroup` |
| `empty(...)` | Dot field, one line, one action | `div` plus `dot_field` |
| `dot_field(color)` | Grid of small dots that fades out from a corner | `canvas` painting quads |

**Buttons** come from gpui-kit's `Button`, with colours set in the theme:
- Heights are 24, 28, 32 and 40 px.
- Primary is `accent` with `on-accent` text.
- Secondary is `card` with a `border-2` edge and e1 elevation.
- Ghost is transparent with a `hover` background.
- Danger is ghost with `danger` text, and filled only inside a confirmation dialog.

**Charts** come from gpui-kit:
- dashed `border-1` grid lines;
- axes in `caption` and `text-3`;
- lines 2 px in `accent`, over an area that fades from the accent at 18% to nothing;
- bars in subject colours with 4 px top corners and labels in `caption`;
- the radar chart for the Mains self-evaluation.

## 6. Screens

The sketches show structure. The colours and type are in the mockup.

### Today

```
┌ ☀ Today ───────────────────────────────────── [⚙ Plan] [▶ Start next task ↵] ┐
│  TUESDAY, 17 NOVEMBER                                  ⋰⋰ dot field ⋰⋰       │
│  Good morning                                               ╭─────╮          │
│  (🔥 10-day streak) (187 days to Prelims) (61 of 90 min)     │ 67% │ of plan  │
│                                                             ╰─────╯          │
│ ┌ Today's plan · 2 of 5 · 67 min  ▬▬▬▬▬▬░░░░ ┐ ┌ Mentor · 3 ─────────────────┐ │
│ │ ☑ ▣ Read the brief, 16 Nov      25m  Quiz │ │ ↘ Money and banking: 33%    │ │
│ │ ☑ ▣ Review 15 mistakes          15m       │ │   over 18 Q  [Note] [Drill] │ │
│ │ ☐ ▣ Continue: Inflation          5m  Read │ │ ± Guesses cost 0.7 marks    │ │
│ │ ☐ ▣ Drill: Money and banking    10m Start │ │   a test   ▬▬▬|░░           │ │
│ │ ☐ ▣ Mains answer, GS-1 10 marks 12m Write │ │ ↗ Attempt more: +0.8 each   │ │
│ └────────────────────────────────────────────┘ └─────────────────────────────┘ │
│ ┌ Continue reading ──┐ ┌ Question of the day ───┐ ┌ This week ─────────────┐ │
│ │ (GS-3 · Economy)   │ │ Which material ...     │ │ ▇ ▃ ▁ ▁ ▁ ▁ ▁  - goal  │ │
│ │ Inflation and ...  │ │ [A Copper][B Germ.]    │ │ M T W T F S S          │ │
│ │ ▬▬▬▬▬▬▬▬▬░ 1m left │ │ [C Alum.] [D Silicon]  │ │ 1 h 23 m today         │ │
│ └────────────────────┘ └────────────────────────┘ └────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────────────┘
```

- The greeting band has the date as a caption, "Good morning" (or afternoon or evening) in `display`, and status chips. The day ring sits on the right, over a dot field tinted with the accent.
- **Plan rows** are 44 px:
  - a checkbox;
  - a 24 px tile in the subject colour with the task kind's icon;
  - the title, and a meta line with the subject and size;
  - the minutes in tabular figures, then the row's action.
  - A done row turns `text-3`. A bar of one segment per task sits in the card title.
- **"Start next task"** in the header opens the first open task. Enter triggers it.
- **Mentor insights** have an icon, a one-line headline, a supporting number and at most two actions. The diagnostic offer, shown once, replaces the greeting band with a card on a dot field.

### Notes

```
┌rail┐┌ Notes › GS-2 · Polity › Fundamental Rights  [Read|Distilled]   Aa ✎ ⌑ ◨ ┐
│●●● ││ ⌕ Filter notes   │        (GS-2 · Polity)            │ Outline PYQs 8  │
│ ☀  ││ [All|Unread|Saved]│        Fundamental Rights         │ ▍1 Quick anchor │
│ ▯  ││ GS-1              │        Articles 12 to 35          │  2 Six groups   │
│ ✎  ││ ○ Temple archit…  │  6 min · 8 PYQs · asked 2025      │  3 Who holds…   │
│ ▤  ││   ● History · 7m  │  ───────────────────────────────  │  4 Article 21   │
│ ▥  ││ GS-2              │  1. Quick anchor                  │ ┌ Reading ────┐ │
│    ││▐◉ Fundamental R…  │  Part III of the Constitution     │ │ ◔ 40%       │ │
│ ↻  ││   ● Polity · 6m   │  guarantees six groups of rights… │ │ [Mark read] │ │
│ ✍  ││ ○ Panchayati Raj  │  ┌ ⚠ The trap ──────────────────┐ │ └─────────────┘ │
│ ☰  ││   ● Polity · 5m   │  │ Article 21 protects every …  │ │ ┌ Practise ───┐ │
│ ▤  ││ GS-3              │  └──────────────────────────────┘ │ │ 8 questions │ │
└────┘└──────────────────────────────────────────────────────────────────────────┘
```

- **List:** a filter field, the segments All, Unread and Saved, and groups by paper. Each row has a status icon, a one-line title and a meta line with the subject dot and minutes. The open note has an `accent-soft` row.
- **Reader:**
  - a subject chip, the title in `title-1`, the subtitle in serif, and an "at a glance" meta line;
  - the body in `reader` type, headings in `title-2`, and highlights in `highlight`;
  - tables with hairline rows and a `sunken` head;
  - "The trap" as an attention callout.
- **The end of a note** has "Mark as read" (primary, which counts towards coverage), "Practise 10 questions" and the linked questions as compact cards.
- **Inspector tabs:** Outline (the current section gets a 2 px accent bar), Questions and Highlights. A Reading card holds the progress ring and "Mark as read", so it can be reached without scrolling.
- **Aa menu:** text size (13 to 24), width (680 or 920), and Serif or Sans.

### Practice

- The header has "Practice", the bank size as a chip, and "Custom test" as a secondary action.
- **Resume banner**, only when a test is unfinished: its name, a progress bar, the time left and "Resume ↵".
- **Formats:** four tiles in a row (Micro 10 Q · 12 m, Mini 30 · 36 m, Full mock 100 · 2 h, Personalised 20 · 24 m). Each has an icon, a title, a meta line and one line of help. The whole tile is the button, and it lifts to e1 on hover.
- **Subject drills:** a 3-column grid of subject cards. Each has a top edge in the subject colour, the name, the question count, the student's accuracy as a bar, and "Drill". A card the bank cannot fill is greyed out.
- **Past papers:** one row per year with "Read with answers" and "Sit as mock".
- **Recent results:** a compact table with date, test, net score and accuracy, each row opening its result.

### Exam room (focus mode)

```
┌ Subject drill · Polity   Question 5 of 20      ⏱ 22:30      Save and leave  [Submit ⌘↵] ┐
│ ▬▬▬▬▬▬░░░░░░░░░░░░░░░ progress under the header                                         │
│        (Polity) (UPSC Prelims 2025)                         │ Questions                 │
│        With reference to the Indian polity, consider …      │ [1][2][3][4][5]           │
│          I.  The Governor of a State is not answerable …    │ [6][7][8][9][10]          │
│          II. No criminal proceedings shall be …             │ …                         │
│        Which of the statements given above are correct?     │ ■ answered 4  ■ marked 1  │
│        ┌ (A) I and II only ─────────────────────── S ┐      │ □ skipped 1  · not seen 14│
│        ┌ (B) II and III only   selected ─────────── S ┐     │                           │
│        ┌ (C) I and III only ────────────────────── S ┐      │ Keys                      │
│        ┌ (D) ~~I, II and III~~ struck out ──────── S ┐      │ A–D answer · M mark       │
│        How sure are you?  [Sure 1][50-50 2][Guess 3]        │ ←/→ move · ⌘↵ submit      │
├──────────────────────────────────────────────────────────────┴───────────────────────────┤
│ [⚑ Mark for review M]  Clear                                       [← Previous] [Next →] │
└──────────────────────────────────────────────────────────────────────────────────────────┘
```

- The question column is 720 px with the stem in 17 px Inter. Options are 52 px rows: a 28 px letter badge, the option text, and the strike-out control at the far right, shown on hover or with S.
- A selected option gets an `accent` ring, an `accent-soft` fill and a filled badge. A struck option drops to 45% opacity with a line through it.
- The **timer pill** turns `attention` with 5 minutes left and `danger` with 1 minute left.
- **Palette cells** are 40 px squares with 8 px corners:
  - not visited: `border-1` with `text-4` numbers;
  - visited but blank: `border-2`;
  - answered: an `accent` fill;
  - marked: an `attention` fill;
  - answered and marked: an accent fill with an attention corner dot;
  - current: a 2 px `text` ring.

### Result and review

- The **hero** has the net score as a ring (marks over maximum) in `display` figures, then right, wrong and blank as status chips, the accuracy, and the time used.
- **Breakdown:** marks by subject (horizontal bars in subject colours), by confidence (Sure, 50-50 and Guess cards with the marks won or lost) and time per question (a strip against the 72-second budget).
- **Review:** filter tabs (All, Wrong, Marked, Guessed). Each question is a card showing the chosen and correct options, the explanation, the trap callout and "Open note".

### Current affairs

- The **header** has a date stepper "‹ Mon 16 Nov ›" that opens the calendar popover, then Full | Rapid, then "November issue". On the right are "2 of 5 read" as a ring chip and "Take quiz · 5 Q" as the primary action.
- The **list** groups stories by subject, each with a subject dot. A title takes at most two lines. The day's editorials come last.
- The **story reader** has a subject chip, the title in `title-1` and a linked-note chip. Its sections have `caption` labels: Context, Why it matters, Key facts (accent markers), the trap (attention callout), PYQ pattern, the practice question (inline, answer revealed in place), and the Mains question with its answer frame as numbered steps. The footer has "Save to notebook" and "Next story →".
- **Rapid mode** is a feed of compact cards, one per story, with the title and key facts.
- The **monthly issue** is one page with stories grouped by subject, like a magazine contents page, and its quiz at the top.

### Analysis

- The **header** has "Analysis", tabs for Overview, Subjects, Tests, Time and Coverage, and a range menu on the right ("Last 10 tests").
- When demo history is loaded, a thin `sunken` strip says so, with "Remove".
- **Overview:**
  - Four stat tiles: average net score, accuracy, attempt rate and this week's study time, each with a delta and a sparkline.
  - Net score trend (an area chart in accent, 8 columns) beside the guess analysis (4 columns): one row per confidence level, a bar left or right of zero and the marks.
  - Accuracy by subject (horizontal bars in subject colours, weakest first) beside the weakest topics (a table with Drill buttons).
  - A 12-week study-time heatmap across the full width, with cells in five accent steps.
- **Coverage:** one ring per paper, then the subjects of each paper as bars.

### Revision

- The **header** has "Revision", "12 due today" as a chip, and "Start session (12)" as the primary action.
- **Leitner boxes:** five columns for boxes 1 to 5 (gaps of 1, 3, 7, 16 and 35 days). Each shows its card count as a bar, with today's due items picked out. It shows the method at a glance.
- **The mistake book by subject:** subject dots and counts, with each row opening its questions. Then the flashcard decks.
- **Session (focus mode):** one centred card with a progress bar above it. A mistake is asked again as a question. A flashcard is flipped with Space, then marked "Missed 1" or "Got it 2".

### Mains

- The **list** filters by paper (GS-1 to GS-4) and by Answered or New. Rows carry chips for marks, word limit and status.
- **Editor (focus mode):**
  - the question at the top;
  - the timer pill, and a word meter against the 150 or 250 word limit;
  - a writing surface on `card` in serif type;
  - "Saved" with a check, which appears after each autosave.
- **Self-evaluation:** two columns. On one side, the student's answer. On the other, the model answer and a checklist of the points to cover. Below them are the seven dimensions as 0 to 5 segmented controls, with a radar chart that updates as they are scored.
- **History:** a table of answers, and a line per dimension over time.

### Syllabus

- The header shows "Syllabus" with the overall coverage as a ring chip.
- A row of paper cards (Prelims GS, CSAT, GS-1 to GS-4, Essay), each with a coverage bar. Choosing one scrolls the tree to it.
- The **tree** keeps today's behaviour, restyled: 30 px rows, chevrons in `text-3`, status icons, a coverage bar and figures right-aligned in `meta`, and UPSC's wording as an indented quote once a row is open.

### Notebook

- A **filter list** on the left: All, Highlights, Clips, Bookmarks and My notes, with counts. It sits above a search field.
- **Items as cards:** a highlight shows the quoted text with a bar in the highlight colour, its source and its date. My notes open in an editor pane with a title field and a Markdown body.

### Settings

As in Capy: a 640 px centred column with section labels and grouped cards of rows. Each row has its title and help on the left and its control on the right. The sections are Exam, Study, Appearance, Reader and Library and data:
- Appearance has the theme as three small preview cards.
- Reader has Serif or Sans and the text size.
- Library and data covers the progress file, demo history and content problems.

### Search palette

- A 640 px popover at e3 under the top of the canvas.
- A 44 px input with the search icon.
- Grouped results with icons and right-aligned meta.
- A footer of key hints: "↑↓ move · ↵ open · esc close".

## 7. Build phases

The rules from PLAN.md apply: tasks in order, `cargo test`, `cargo fmt --check` and `cargo clippy --all-targets` pass after each, one file per screen, and commit only when asked. Also:

- **Tokens only.** Screen files use `ui::tokens(cx)` and the theme, never a hex value.
- **Kit first.** A visual that two screens share goes in `src/ui.rs`. A one-off stays in its screen.
- **Keep test ids.** The UI test clicks elements by id. When an id has to change, update `tests/screens.rs` in the same task.
- **Both themes.** Each screen's test steps save Paper and Ink screenshots to `target/shots/`, and each task ends by comparing them with the mockup and this file.

### Phase D0: Foundations

- [x] **D0.1 Fonts.** Add Inter (400, 500, 600), Inter Display (600) and Source Serif 4 (400, italic, 600) to `assets/fonts/` with their OFL licence files. Load them at start-up with `text_system().add_fonts` and set the theme's `font_family` to Inter. The test checks that the families resolve.
- [x] **D0.2 Themes.** Rewrite `assets/theme.json` with **Paper** and a new **Ink** from the colour table, using radius 6 and radius_lg 12. Install Ink as the dark theme, as `install_paper_theme` does for Paper today. This alone changes every gpui-kit control.
- [x] **D0.3 Tokens and subject colours.** Start `src/ui.rs` with `tokens(cx)` and `subject_color(topic)`. Unit-test that every subject node in `content/syllabus.toml` gets a colour.
- [x] **D0.4 Kit.** Add the parts in section 5 as the screens in D1 and D2 first need them, not ahead of time.

**Done when** the app runs in Inter with both new themes, and nothing else has moved yet.

### Phase D1: Shell

- [x] **D1.1 Window.** Draw the `chrome` background. Remove the full-width title bar and put the traffic lights in the sidebar's top row. Add the drag regions. Menus and shortcuts stay as they are.
- [x] **D1.2 Sidebar.** Build the 232 px layout: top row, search button, two groups with counts, and the footer with the streak card, focus timer and Settings. The rail is 72 px, and the auto-fold rule stays.
- [x] **D1.3 Canvas and header.** Add the inset canvas and `ui::header`. Move each screen's title, tabs and actions into the header, so content starts directly under it.
- [x] **D1.4 Inspector.** Add the right pane with its toggle and the width rules from section 4. Notes is the first user.
- [x] **D1.5 Palette and banners.** Restyle the ⌘K palette as specified. The "progress is not being saved" banner becomes a strip in `danger` across the top of the canvas.

**Done when** every screen opens inside the new shell in both themes, the window drags from the top, the sidebar folds to the rail and back, and `cargo test` passes.

### Phase D2: Screens

The order follows a student's day. Each task restyles one screen file to its spec in section 6, using the kit, and updates its test steps and screenshots.

- [x] **D2.1 Today** (`today.rs`).
- [x] **D2.2 Notes** (`notes.rs`): list, reader, inspector, and the Aa menu with the new `reader_font` setting.
- [x] **D2.3 Practice** (`practice.rs`): hub, custom test, past papers.
- [x] **D2.4 Exam room** (`exam.rs`), in focus mode.
- [x] **D2.5 Result and review** (`review.rs`), which also draws a single question and a past paper read with its answers.
- [x] **D2.6 Current affairs** (`current_affairs.rs`): Full, Rapid, calendar, monthly issue.
- [x] **D2.7 Analysis** (`analysis.rs`): all five tabs, with charts in the new style.
- [x] **D2.8 Revision** (`revision.rs`): Leitner boxes, mistake book, session.
- [x] **D2.9 Mains** (`mains.rs`): list, editor, self-evaluation with the radar chart, history.
- [x] **D2.10 Syllabus** (`syllabus.rs`).
- [x] **D2.11 Notebook** (`notebook.rs`).
- [x] **D2.12 Settings** (`settings.rs`), with the theme preview cards and reader options.
- [ ] **D2.13 Empty states.** Each screen with nothing to show gets `ui::empty` with one line and one action. (Today, Notes and Analysis have plain one-line empty states; `ui::empty` with the dot field is not built.)

**Done when** every screen matches its spec at 1280 × 820 in both themes.

**As built (D0 to D2).** `cargo test` drives every screen in the new shell and saves Paper and Ink screenshots. Where the build differs from this plan:

- **Sidebar.** It is still gpui-kit's `Sidebar`, so the UI test's ids stay. Its rows are the component's own 26 px, not 30. When folded, the component is 48 px wide inside a 72 px column that holds the traffic lights. The rail stacks Back and Forward, then the fold button and the theme switch. The theme switch moved into the top row because the title bar is gone.
- **Header.** Each screen draws its own header with `ui::header`, a restyled gpui-kit `TitleBar`, so the header drags and double-clicks like a title bar.
- **Type.** The theme's `font.size` is 15, so gpui-kit's controls get 13 px text and 30 px rows. Kit parts set their sizes in pixels.
- **Reader.** The body is set in Source Serif 4 and so are its headings. A gpui-kit `TextView` cannot give headings a font of their own, and each section is one view whose text the highlights index into. There is no Aa menu and no Serif or Sans switch yet: the reader is always serif, and the text size and the 920 px wide column are buttons in the header.
- **Inspector.** Only Notes has one: the outline, a reading ring with Mark as read, and a drill on the note. It does not hide itself below 1100 px yet (D3.2).
- **Exam room.** It gained keys 1, 2 and 3 for the confidence tag and ⌘↵ to submit, so the room can show those hints truthfully. There is no S key for strike-out, so no S hint is shown.
- **Left out for now.** These are left out until there is a reason to add them:
  - the Enter key for Today's "Start next task";
  - deltas and sparklines on the Analysis tiles;
  - a text filter above the notes list (it has the All, Unread and Saved segments);
  - the radar chart on the Mains score sheet;
  - the palette's key-hint footer;
  - Practice's "Custom test" header button (the custom drill is on the page).
- **Highlights** use the `highlight` token instead of their own amber.

### Phase D3: Polish

- [x] **D3.1 Ink pass.** Check every screenshot in dark mode for contrast and stray light surfaces. (The screenshot test covers Today, Settings and Notes in Ink; they hold up. The other screens use the same tokens.)
- [ ] **D3.2 Small window.** At 900 × 600 the width rules hold, with nothing clipped and no horizontal scroll.
- [ ] **D3.3 Motion.** Add hover and press transitions, and the inspector and sheet slide. Check that reduce-motion turns them off.
- [ ] **D3.4 Keyboard.** Every button with a shortcut shows it. Focus rings are visible in the accent, and Tab order follows reading order.
- [ ] **D3.5 Review.** Put all screenshots in both themes side by side and fix anything that has drifted from this file.

**Done when** the screenshots in both themes and both window sizes match this file, and `cargo test` passes.

## 8. Decisions, with the defaults I will use

1. **Accent:** Peacock, `#006D99` in light and `#42ACD2` in dark. It is one token to swap.
2. **Reader font:** Source Serif 4 by default, with Inter one click away in the Aa menu.
3. **Translucent sidebar** (macOS vibrancy): off. It is solid `chrome`, as Capy's is. It can be revisited after D3.
4. **Dot field** on Today and empty states: on and subtle. It is the one decorative touch.
5. **Exam room theme:** follows the app's theme. It is not forced to dark.
6. **Fonts are embedded.** This adds about 2 MB to the binary and makes the app look the same on macOS, Windows and Linux.
