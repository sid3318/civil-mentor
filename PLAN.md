# Civil Mentor: build plan

Status: Phase 0 (the shell) and Phase 1 (notes and the full UPSC syllabus) are built and tested. Phases 2 to 6 are not started. Plan written 14 Nov 2026, updated 15 Nov 2026.

A desktop study companion for UPSC CSE aspirants. It plans the student's day, gives them the notes and current affairs to read, tests them in an exam-like room, and turns the results into the next thing to study.

- Stack: Rust + GPUI, through the `gpui-kit` crate.
- Feature reference: deepmentor.co, reviewed on the date above. We take its features, not its content or branding.
- Content: all our own. The first build ships original dummy data.

## 1. How the app works

### For the student

The app runs one loop: **Plan → Learn → Practise → Analyse → Revise.**

1. **Plan.** The Today screen shows a plan sized to the student's daily study time (default 90 minutes): read today's brief, continue a note, revise due cards, take a drill on a weak topic.
2. **Learn.** The student reads notes (full or distilled) and the daily current-affairs brief. Reading is tracked.
3. **Practise.** Timed tests run in a distraction-free exam room with UPSC marking.
4. **Analyse.** Every answer feeds the analysis: accuracy by subject and topic, speed, guess accuracy, syllabus coverage.
5. **Revise.** Wrong answers and flashcards come back on a spaced schedule.
6. Tomorrow's plan is rebuilt from what the analysis found.

The spine is the **syllabus**. Every note, question, news story and flashcard carries syllabus topic IDs. A topic therefore leads to everything on it, and a weak score in a topic leads straight to its note and a drill.

### Under the hood

```
┌───────────────────── civil-mentor (one Rust binary) ─────────────────────┐
│  UI: GPUI + gpui-kit components                                          │
│  App shell (sidebar, title bar, search) + one screen at a time           │
│        │ reads              │ reads and writes         │ asks            │
│        ▼                    ▼                          ▼                 │
│  Content (read-only)   Store (SQLite)            Mentor (rules)          │
│  syllabus, notes,      answers, attempts,        reads both, returns     │
│  questions, briefs,    reading, reviews,         today's plan, insights, │
│  flashcards            highlights, study time    question mix, reviews   │
│        ▲                    ▲                                            │
│  content/ files        app-data folder                                   │
│  (Markdown + TOML)     (one .db file)                                    │
└──────────────────────────────────────────────────────────────────────────┘
```

- **One binary, no server.** Everything works offline.
- **Content** is plain files, loaded into memory at startup and validated. A question that points to a missing note or an unknown topic is reported, not silently dropped.
- **Store** is one SQLite file in the OS app-data folder. Every answer is written the moment it is given, so a test survives a crash and can be resumed.
- **Mentor** is a set of plain functions. They read content and store, and return today's plan, the insights, the personalised question mix and the review dates. It is rule-based, deterministic and unit-tested. AI can be added later behind the same functions.
- **UI.** GPUI draws the interface on the GPU. There is no web view. Each screen is a view over state, and when the state changes the view re-renders. `gpui-kit` supplies the ready-made parts: sidebar, tabs, tables, charts, Markdown view, calendar, command palette.
- **Linking.** Every screen opens things through one `open(target)` call on the shell (a note, a question, a story, a topic, a drill). That is what lets an insight card open a note or start a drill, with back and forward history.

### What happens when a student takes a test

1. In Practice the student picks "Mini test: Economy". The mentor selects 30 question IDs, weighting unseen and previously wrong questions up.
2. The exam room opens: one question at a time, a live clock, a question grid, mark-for-review and a confidence tag.
3. Each answer is saved immediately.
4. On submit the score is computed (+2 for right, minus one-third of that for wrong, 0 for blank). The result screen shows the score, the subject breakup, and every question with its answer, explanation, the trap, and the note behind it.
5. Wrong answers join the mistake queue. Topic accuracy updates. Analysis and tomorrow's plan change.

## 2. Features

### Five engines, many sections

deepmentor has about 20 sections, but they are views over a few engines. We build the engines. Most sections then cost content, not code.

| Engine | What it powers |
|---|---|
| Reader (Markdown documents) | Notes, distilled notes, cheatsheets, guides, books, concepts, editorials, monthly compilations |
| Test engine (question bank) | Drills, mocks, PYQ papers, daily news quiz, personalised mix, mistake re-tests, CSAT |
| Daily feed | Current-affairs briefs by date |
| Tracker | Reading progress, streaks, study time, analysis |
| Mentor | Today's plan, insights, revision schedule. Later: AI chat and AI answer evaluation |

### Feature map

| deepmentor | Civil Mentor | Phase |
|---|---|---|
| Search (⌘K) | Command palette over notes, questions and stories | 0 |
| Notes for GS-1 to GS-4: Read, Book and Distilled modes, outline, "at a glance", linked PYQs, text size and width | Same, with Read and Distilled modes. Book mode is left out for now | 1 |
| Syllabus hub | Syllabus tracker with a status per topic and coverage per paper | 1 |
| Practice: micro (10), mini (30), full mock (100), subject drill (20), custom drill, personalised mix, PYQ test | Same set, plus confidence tagging and option strike-out | 2 |
| Prelims PYQs by year: read, practise, mock | PYQ papers by year: read with answers, or sit as a mock | 2 |
| Exam room: live clock, question grid, mark for review, instant explanations, UPSC marking | Same, plus resume after a crash | 2 |
| Progress: streak, items studied, tests taken, scores by subject, weak areas | Analysis dashboard and mentor insights | 3 |
| Daily plan and Study dashboard: countdown, week view, continue reading, question of the day, focus session | One Today screen | 3 |
| Current affairs: daily brief, Full and Rapid modes, story sections, editorials, calendar archive, monthly issue, daily test | Same structure | 4 |
| Revision: decks, condensed notes, cheatsheets | Mistake book and flashcards on spaced repetition. Cheatsheets are notes of kind `cheatsheet` | 5 |
| My notes: highlights, clips, embeds | Notebook: highlights, clips, bookmarks, own notes | 5 |
| Mains PYQs with model answers | Same, with a timed answer editor and word count | 6 |
| AI answer evaluation: 7 scored dimensions, what to fix, points missed | Self-evaluation on the same 7 dimensions against the model answer. AI marking later | 6 |
| Ask Mentor: AI chat with sources, voice | Later. Needs an LLM API reached through your own server | Later |
| Optionals, books, concepts, guides, reference tables, essay, interview, government data | Content only: the same reader with a new `kind`. No new engine | When content exists |
| CSAT lessons and practice | Practice runs on the test engine with CSAT marking. Interactive lessons later | Later |
| Atlas: interactive maps with marked places | Later. It is its own engine (map drawing and place markers) | Later |
| Sign-in, plans, credits, rewards, referrals | Later. Needs a backend. Version 1 is one local profile | Later |
| Explore graph, Journey | Skipped. The syllabus tracker covers this | Skip |

### Additions I recommend

1. **Confidence tagging.** The student tags each answer Sure, 50-50 or Guess. Analysis then shows whether guessing gains or loses marks under negative marking. This is the most useful single number for Prelims strategy.
2. **Mistake book.** Every wrong answer is re-asked after 1, 3, 7, 16 and 35 days until it sticks.
3. **Option strike-out.** The student can cross out options in the exam room, the way elimination is done on paper.
4. **Diagnostic test on first launch.** Twenty questions across subjects seed the weak-area model on day one.
5. **High-yield unread.** A list of topics with many past questions that the student has not read yet.
6. **Keyboard first.** ⌘K search, ⌘1 to ⌘5 for sections, A to D to answer, M to mark, arrow keys to move.

## 3. Layout

Rules for every screen:

- A persistent left sidebar holds five primary sections and four secondary ones. It collapses to an icon rail while reading.
- Notes and Current affairs use the same three panes: a list, a reader, a context panel.
- The exam room takes the whole window. No sidebar, no title bar.
- The reader column has a maximum width of about 720 px, with a light "paper" theme by default and a dark theme available.

Shell and Today:

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ < >    [ Search notes, questions, news ...  Cmd+K ]     191 days | streak 12 │
├─────────────┬────────────────────────────────────────────────────────────────┤
│ Today       │ Saturday, 14 Nov                                               │
│ Notes       │ ┌ Today's plan - 1 of 5 - 95 min ────┐ ┌ Mentor ─────────────┐ │
│ Practice    │ │ [x] Read today's brief      25 min │ │ Parliament: 42% in  │ │
│ Current     │ │ [ ] Continue: Fund. Rights  20 min │ │ your last 3 tests.  │ │
│ Analysis    │ │ [ ] Revise 14 due cards     10 min │ │ [Open note] [Drill] │ │
│ ─────────── │ │ [ ] Mini test: Economy      24 min │ ├─────────────────────┤ │
│ Revision    │ │ [ ] Write one Mains answer  16 min │ │ Guesses cost you 9  │ │
│ Mains       │ └────────────────────────────────────┘ │ marks per test.     │ │
│ Syllabus    │ ┌ Continue reading ──┐ ┌ Question ───┐ └─────────────────────┘ │
│ Notebook    │ │ Fundamental Rights │ │ of the day  │ ┌ This week ──────────┐ │
│             │ │ 40% - 12 min left  │ │ ...         │ │ M T W T F S S       │ │
│ Focus 25:00 │ │                    │ │             │ │ # # # . . . .       │ │
│ Settings    │ └────────────────────┘ └─────────────┘ └─────────────────────┘ │
└─────────────┴────────────────────────────────────────────────────────────────┘
```

Notes (sidebar collapsed to a rail):

```
┌────┬──────────────────────┬──────────────────────────────────┬───────────────┐
│ T  │ GS-2                 │ [Read | Distilled]   Aa   Save   │ On this page  │
│ N  │ v Polity (31)        │                                  │  1 Anchor     │
│ P  │   * Fund. Rights     │ Fundamental Rights               │  2 Article 14 │
│ C  │   o DPSP             │ 26 min - 12 PYQs - asked 2025    │  3 Article 19 │
│ A  │   . Parliament       │                                  │  4 Article 21 │
│    │   . Judiciary        │ The text of the note, with the   ├───────────────┤
│    │ > Governance (17)    │ student's own highlights.        │ PYQs (12)     │
│    │ > Social justice     │                                  │ Highlights (3)│
│    │ > IR (21)            │ [Mark as read]  [Practise 10 Q]  │               │
└────┴──────────────────────┴──────────────────────────────────┴───────────────┘
 rail  notes tree             reader                              context panel
```

Exam room (whole window):

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ Mini test: Economy          Question 14 of 30          21:48 left   [Submit] │
├───────────────────────────────────────────────────┬──────────────────────────┤
│ With reference to the Indian economy, consider    │  1  2  3  4  5  6  7  8  │
│ the following statements:                         │  9 10 11 12 13[14]15 16  │
│  1. ...                                           │ 17 18 19 20 21 22 23 24  │
│  2. ...                                           │ 25 26 27 28 29 30        │
│ Which of the statements given above is correct?   │                          │
│                                                   │ answered 13 - marked 3   │
│  (A) 1 only                                       │ not visited 14           │
│  (B) 2 only                                       ├──────────────────────────┤
│  (C) Both 1 and 2                                 │ How sure are you?        │
│  (D) Neither 1 nor 2                              │ [Sure] [50-50] [Guess]   │
├───────────────────────────────────────────────────┴──────────────────────────┤
│ [Mark for review]  [Clear]                             [< Previous] [Next >] │
└──────────────────────────────────────────────────────────────────────────────┘
```

Analysis:

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ Analysis      [Overview] [Subjects] [Tests] [Time] [Coverage]                │
├──────────────────────────────────────┬───────────────────────────────────────┤
│ Net score, last 10 tests (line)      │ Accuracy by subject (bars)            │
│                                      │ Polity      ######....  61%           │
│                                      │ Economy     ####......  42%           │
├──────────────────────────────────────┼───────────────────────────────────────┤
│ Weakest topics                       │ Guess analysis                        │
│ Parliament      42%  19 Q   [Drill]  │ Sure    82% right   +96 marks         │
│ Inflation       45%  11 Q   [Drill]  │ 50-50   51% right   +11 marks         │
│ Monsoon         48%   8 Q   [Drill]  │ Guess   24% right   -17 marks         │
├──────────────────────────────────────┴───────────────────────────────────────┤
│ Study time, last 12 weeks (heatmap)        Syllabus read: GS-1 34%  GS-2 51% │
└──────────────────────────────────────────────────────────────────────────────┘
```

Current affairs:

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ < 14 Nov 2026 >  [Calendar]   [Full | Rapid]        7 stories - 2 read       │
├───────────────────────────┬──────────────────────────────────────────────────┤
│ Polity and governance     │ New data protection rules notified               │
│  * Data protection rules  │ GS-2 - Governance      Note: Right to privacy    │
│  . Delimitation panel     │                                                  │
│ Economy                   │ Context ...                                      │
│  . Repo rate held         │ Why it matters ...                               │
│  . Exports hit a record   │ Key facts ...           The trap ...             │
│ Environment               │ PYQ pattern ...                                  │
│  . New Ramsar sites       │ Practice question (answer inline)                │
│ Editorials (2)            │ Mains question + answer frame                    │
│ [Take today's quiz - 10 Q]│ [Save to notebook]              [Next story >]   │
└───────────────────────────┴──────────────────────────────────────────────────┘
```

## 4. Tech

### Dependencies

| Piece | Choice | Why |
|---|---|---|
| UI | `gpui-kit` 0.9.1 | One dependency. It bundles a weekly GPUI snapshot (`gpui-pre` 0.3.12) and `gpui-component` (60+ components): sidebar, tabs, data table, charts (line, bar, area, pie, radar), a Markdown view with text selection and range highlights, calendar, command palette, settings page. Plain `gpui` on crates.io is stale (0.2.2, October 2025) |
| User data | SQLite through `rusqlite` (bundled) | One file, crash-safe, and analysis is a few `GROUP BY` queries |
| Content parsing | `serde` + `toml`, and `serde-saphyr` for the header block of a note | Authors edit plain files that can be reviewed in git |
| Everything else | Standard library, or crates `gpui-kit` already pulls in (`chrono`, `anyhow`, `rust-embed`, `dirs`) | No extra build cost |

`Cargo.lock` started from the lock file that ships inside `gpui-kit` 0.9.1, so every shared crate is at the version the kit was tested with. The kit's `test-support` feature does not resolve otherwise: it pins exact versions of `taffy` and others.

### This Mac, checked 14 Nov 2026

- **Rust is now 1.100.0**, updated from 1.88.0, which two dependencies refused.
- **Xcode's Metal Toolchain is not needed.** It is still not installed, and the app builds, renders and opens its window without it, because `gpui-kit` compiles shaders at runtime.
- **A clean build takes about two minutes.** `target/` is about 3.3 GB with both the app and the test build in it. Debug info for dependencies is switched off to keep it that small.
- **8 GB RAM, 17 GB free disk.** Freeing some disk space is still worth doing.

### Project structure

As built in Phases 0 and 1:

```
Cargo.toml
assets/theme.json        the "Paper" light theme
content/                 the content pack (dummy now, ours later)
  syllabus.toml
  notes/gs2/polity/fundamental-rights.md
  questions/polity.toml
  current-affairs/2026-11-14.toml
src/
  main.rs                calls run()
  lib.rs                 start-up: app state, theme, icons, window
  app.rs                 shell: title bar, sidebar, shortcuts, search palette, open(target), back and forward
  content.rs             content types, loader, validation, search, note sections
  store.rs               SQLite schema, migrations, settings, reading records
  notes.rs               the Notes screen: list, reader, outline, linked questions
  syllabus.rs            the Syllabus screen, topic status and coverage
  settings.rs            the Settings screen
  screens.rs             stand-in screens, replaced phase by phase
tests/
  screens.rs             drives the shell offscreen and saves a PNG of every screen
```

Still to come, each with its phase: `mentor.rs` (plan, insights, question mix, review dates, scoring), one file per real screen, and `content/mains/` and `content/flashcards/`.

One crate, no workspace. The app is a library plus a three-line `main.rs` so the tests can drive it.

### How it is tested

- `cargo test` runs the unit tests for the content loader and the store, then `tests/screens.rs`.
- `tests/screens.rs` opens the real shell in an offscreen window with the real renderer. It clicks every sidebar item, uses the shortcuts, searches, changes each setting and checks the result. It saves a PNG of every state to `target/shots/`.
- The content test loads the shipped `content/` folder and fails on any problem, so it doubles as the linter for writers.

### Content format

The syllabus is `content/syllabus.toml`, one line per node:

```toml
topic = [
  { id = "mains", title = "Mains", official = "The Written Examination will consist of 9 papers ..." },
    { id = "mains.gs2", title = "GS-2", official = "General Studies-II: Governance, Constitution, Polity, ..." },
      { id = "mains.gs2.polity", title = "Constitution and polity" },
        { id = "mains.gs2.polity.constitution", title = "Indian Constitution", official = "Indian Constitution—historical underpinnings, ..." },
]
```

- The levels are stage (Prelims, Mains), paper, subject, chapter, topic and subtopic. The trees cover Prelims GS Paper I and CSAT, and Mains Papers A and B, the Essay and GS-1 to GS-4: 1,805 nodes, 1,440 of them at the bottom level. The optional subjects are not in yet.
- `official` is UPSC's wording, copied from Examination Notice No. 05/2026-CSE (Appendix I, Section III), on every node that matches a line of the official syllabus. The breakdown below those lines is ours, cross-checked against the subject-wise syllabus guides of Drishti IAS, Vajiram & Ravi, Vision IAS, Next IAS, ClearIAS and Insights IAS.
- Content is tagged with the most specific nodes it covers in full. A note covers everything under the nodes it is tagged with, so a note on a whole chapter is tagged with the chapter. Notes and stories carry both the Mains node and the Prelims node when both apply. Questions carry Prelims nodes.

A note is one Markdown file:

```markdown
---
id: gs2-polity-fundamental-rights
title: Fundamental Rights (Articles 12 to 35)
topics: [gs2.polity.fundamental-rights]
kind: note            # note | cheatsheet | guide | book | concept
minutes: 26
updated: 2026-11-01
---
## 1. Quick anchor
Full text of the note...

<!-- distilled -->
- The short version, shown in Distilled mode.
```

Questions are TOML, one file per subject:

```toml
[[question]]
id = "polity-0001"
topics = ["gs2.polity.fundamental-rights"]
source = "Civil Mentor"          # or "UPSC Prelims 2021"
stem = """
Consider the following statements:
1. ...
2. ...
Which of the statements given above is correct?"""
options = ["1 only", "2 only", "Both 1 and 2", "Neither 1 nor 2"]
answer = "A"
explanation = "..."
trap = "..."
note = "gs2-polity-fundamental-rights"
```

A day of current affairs is one TOML file with a list of stories. Each story has: title, GS paper, topics, context, why it matters, key facts, the trap, PYQ pattern, a practice question, a Mains question with an answer frame, and linked notes.

### What the store holds

| Table | One row per | Key fields |
|---|---|---|
| `attempt` | test sitting | kind, start, finish (empty while in progress), time allowed, marks for right and wrong |
| `answer` | question in a sitting | question ID, subject, topic, chosen option, correct, confidence, marked, time taken |
| `reading` | note or story opened | first, last and done dates, bookmark. Time spent comes with Phase 3 |
| `review` | flashcard or mistake | box (1 to 5), due date |
| `mark` | highlight or clip | item, text range, the student's comment |
| `my_note` | note the student wrote | title, body |
| `session` | block of study time | day, kind, seconds |
| `plan_task` | task in a day's plan | kind, target, minutes, done |
| `mains_answer` | written answer | question, text, self-scores |
| `setting` | setting | exam date, daily minutes, theme |

### Mentor rules, version 1

- **Today's plan** fills the daily minutes in this order: today's brief if unread, reviews due today (15 minutes at most), the note in progress or else the next unread note in the least-covered subject, a drill on the weakest topic that has enough attempts, and a Mains answer on alternate days. The plan is frozen for the day once made.
- **Personalised mix** samples questions by weight: previously wrong 2, unseen 1, previously right 0.2, and 1.5 times that for weak topics.
- **Review dates** use five boxes with gaps of 1, 3, 7, 16 and 35 days. Right moves a card up one box. Wrong sends it back to box 1.
- **Insights**: weakest topics (under 50% over at least 8 questions), marks won or lost by confidence level, seconds per question against the 72-second budget, attempts against net score, high-yield unread topics, and study time against the goal.

Each of these is a plain function with a unit test.

## 5. Build phases

Each phase ends with an app that runs and can be clicked through.

| Phase | Scope | Done when |
|---|---|---|
| 0. Shell (done) | Update Rust. Window, sidebar, title bar, light and dark theme, routing. Content loader with validation. Store and settings. Dummy content. ⌘K search | The app launches, every sidebar item opens its screen, dummy content loads with no validation errors, and `cargo test` passes the loader and store round trip |
| 1. Notes and syllabus (done) | Notes tree, reader (Read and Distilled, outline, text size and width, at a glance, linked PYQs), mark as read, bookmark. Syllabus tracker | A note can be read in both modes and marked read, and the coverage figure changes |
| 2. Practice | Practice hub, drill builder, exam room, result and review, PYQ papers by year | A drill can be sat against the clock, quit, resumed, submitted and reviewed. A unit test checks the score against a hand-worked case |
| 3. Analysis and Today | Analysis dashboard, mentor rules, Today screen, streak, countdown, demo-history loader | With demo history loaded the charts draw, and the plan and insights appear and are covered by tests |
| 4. Current affairs | Brief by date, Full and Rapid modes, story reader with inline question, editorials, calendar, daily quiz | A brief can be read and its quiz taken, and both show up in Today and Analysis |
| 5. Revision and notebook | Mistake book, flashcards, review session, highlights, clips, own notes, focus timer | A wrong answer comes back on its due date. A highlight survives a restart |
| 6. Mains | Mains questions with model answers, timed editor with word count, self-evaluation on 7 dimensions, history | An answer can be written, scored and seen in the history |

The first build is Phases 0 to 4: the four pillars and the Today plan. Phases 5 and 6 follow.

### Phase 0 as built

All four "done when" conditions are met and checked by `cargo test`. What differs from the plan above:

- **Content is a starter pack**, not the full dummy set: 83 syllabus topics, 4 notes, 30 questions and 2 daily briefs. Each later phase adds the rest of its own dummy data.
- **Mains questions and flashcards have no loader yet.** They arrive with Phases 6 and 5, when the screens that use them are designed.
- **Search opens the section a result belongs to**, not the item. Opening the note, question or story itself comes with Phases 1, 2 and 4, and so do the back and forward buttons.
- **The store has one table, `setting`.** The other tables are added by the phase that needs them, as numbered migrations.
- **The default Prelims date is 23 May 2027.** It is a placeholder for the student to change in Settings.

### Phase 1 as built

The "done when" condition is met and checked by `cargo test`: the UI test opens a note, switches it to Distilled, marks it read and watches GS-2 coverage go from 0 to 1 of 18 topics. What differs from the plan above:

- **The notes list** groups notes by paper and subject, in syllabus order, with bookmarked notes on top. Subjects do not fold yet; add that when there are hundreds of notes.
- **"At a glance" is the line under the title**: reading time, linked questions and the date the note was last updated. "Asked in" years wait for real PYQs, which come with Phase 2's papers.
- **Linked questions sit at the end of the note**, each with its answer behind a button, because the side panel is too narrow for a question. The side panel holds the outline, made from the note's `## ` headings, and its last entry jumps to the questions. A question is linked when it names the note or shares a topic with it.
- **Text size** (13 to 24 px) and **column width** (720 or 960 px) are saved as settings, so the reader remembers them.
- **Bookmarks are a column on `reading`**, not rows in `mark`. Only notes and stories are bookmarked, and both already have a reading row. `mark` is left to Phase 5 for highlights and clips.
- **Reading records** the first and last day a note was opened and the day it was marked read. Time spent waits for Phase 3, which needs study time anyway.
- **The syllabus was rebuilt** from the official notice into the full Prelims and Mains trees described under "Content format", replacing the 83-topic starter. The Syllabus screen shows it as a tree that folds: papers first, each showing its coverage, then subjects, chapters, topics and subtopics. UPSC's wording appears under a row once it is opened.
- **Topic status comes from the notes that cover it**: no notes yet, not started, in progress, or read once every such note is read. Coverage is read topics over all bottom-level topics, including those with no notes yet, so it is an honest share of the syllabus. There is no manual status per topic; add one if students want to tick off topics studied elsewhere.
- **Search and links open the note itself**, and the title bar has back and forward buttons (also Cmd+[ and Cmd+]). Questions and stories still open their section until Phases 2 and 4.
- **The sidebar folds to an icon rail on the Notes screen** and unfolds when the student leaves, unless they opened or closed it themselves.
- **Content**: 12 notes, 3 per GS paper. Existing questions and stories link to the new notes, and everything is tagged with the new syllabus ids.

Dummy data for the first build, all original and marked as sample: 12 notes (3 per GS paper), about 100 MCQs across 6 subjects, 7 days of briefs with 5 stories each, 30 flashcards, 10 Mains questions with model answers, and a "load demo history" action that adds 30 days of made-up attempts so Analysis is not empty.

Later, in no fixed order: AI chat and AI answer evaluation, a content sync server, accounts and plans, atlas, optionals, CSAT lessons, Hindi, Windows and Linux builds.

## 6. Risks

1. **GPUI is pre-1.0 and changes weekly.** Versions are pinned through `Cargo.lock` and upgraded on purpose.
2. **Desktop only.** GPUI runs on macOS, Windows and Linux, but each needs its own build and testing. This code gives no phone or web app.
3. **Content is the real work.** deepmentor lists 792 notes, 3,524 solved PYQs and 579 daily briefs. Daily current affairs also needs a delivery channel (a sync server) before real students use the app.
4. **AI features need a server.** The API key must live on your server, never inside the app.
5. **This Mac is tight on disk and memory.** The first build will be slow.

## 7. Decisions, with the defaults I will use

1. The mentor is rule-based in version 1. AI chat and AI answer evaluation come later.
2. The first build is Phases 0 to 4.
3. macOS first. Windows is tested before launch.
4. English only, one local profile, no sign-in.
5. Name "Civil Mentor", light paper theme by default.
6. Content is Markdown for notes and TOML for questions and news. If the writers prefer a spreadsheet for questions, an importer is cheap to add.
