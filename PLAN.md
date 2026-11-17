# Civil Mentor: build plan

Status: Phase 0 (the shell), Phase 1 (notes and the full UPSC syllabus), Phase 2 (practice: tests, the exam room, results and past papers), Phase 3 (Analysis, the Today plan, study time and demo history), Phase 4 (current affairs: daily briefs, the story reader, the daily and monthly quizzes) Phase 5 (revision: review dates, the mistake book and flashcards, the review session, highlights and clips, the Notebook and the focus timer) and Phase 6 (Mains: questions with model answers, the timed editor, self-evaluation and the history) are built and tested, so every phase of this plan is built. What comes next is under "Later". Plan written 14 Nov 2026, updated 17 Nov 2026.

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
| AI answer evaluation: 7 scored dimensions, what to fix, points missed | Self-evaluation on 7 dimensions against the model answer (see Phase 6). AI marking later | 6 |
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

As built in Phases 0 to 6:

```
Cargo.toml
assets/theme.json        the "Paper" light theme
content/                 the content pack (dummy now, ours later)
  syllabus.toml
  notes/gs2/polity/fundamental-rights.md
  questions/polity.toml   sample questions, one file per subject
  questions/upsc-prelims-2025.toml   a real paper with UPSC's answer key
  current-affairs/2026-11-14.toml   one brief per day, 10 to 16 Nov 2026
  flashcards/polity.toml  sample flashcards, one file per subject
  mains/gs2.toml          sample Mains questions with model answers, one file per paper
src/
  main.rs                calls run()
  lib.rs                 start-up: app state, theme, icons, window
  app.rs                 shell: title bar, sidebar, shortcuts, search palette, open(target), back and forward
  content.rs             content types, loader, validation, search, note sections
  store.rs               SQLite schema, migrations, settings, reading records, tests and answers, study time, plans, reviews, marks, own notes, Mains answers, demo rows
  mentor.rs              subjects, scoring, question choice, today's plan, insights, the streak, review dates, the Mains clock, task and trend
  today.rs               the Today screen: plan, continue reading, question of the day, mentor cards, this week
  analysis.rs            the Analysis screen: Overview, Subjects, Tests, Time and Coverage tabs
  demo.rs                30 days of made-up history, added and removed from Settings
  notes.rs               the Notes screen: list, reader, outline, linked questions, the reading clock
  current_affairs.rs     the Current affairs screen: date bar, calendar, stories by subject, editorials, Full and Rapid modes, quizzes, the monthly issue
  practice.rs            the Practice hub: every kind of test, unfinished tests, past papers, results
  exam.rs                the exam room: the clock, the question grid, keys, saving every change
  review.rs              the result and review, one question on its own, a past paper with answers
  revision.rs            the Revision screen: the mistake book by subject and the review session
  marks.rs               highlights and clips: markable text views, the selection, the comment bar
  notebook.rs            the Notebook: highlights, clips, bookmarks and the student's own notes
  mains.rs               the Mains screen: questions by paper and topic, the timed editor, self-evaluation, the history and its trend
  syllabus.rs            the Syllabus screen, topic status and coverage
  settings.rs            the Settings screen
  screens.rs             the page column and counted nouns every screen uses
tests/
  screens.rs             drives the shell offscreen and saves a PNG of every screen
```

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

A question from a real paper has `source = "UPSC Prelims YYYY"`, and those questions make up that year's paper. If UPSC dropped a question from its key, it has `dropped = true` and no `answer`: it is shown in its paper but never scored.

A day of current affairs is one TOML file, named by its date, with a list of stories and a list of editorials. Each story has: a stable id (the date and a slug, never changed once shipped, because the student's reading is kept under it), title, topics (a Prelims subject first, which the screen groups by), context, why it matters, key facts, the trap, PYQ pattern, a practice question, a Mains question with an answer frame, and linked notes. A story's question takes the id `ca-<story id>` unless it names its own; the first six stories keep their old question ids so that answers given before stories had ids still match. An editorial has an id, title, topics, a Markdown body and linked notes. Story and editorial ids are unique across all briefs, and the loader reports a repeat.

A Mains question is a `[[question]]` entry in `content/mains/`, one file per paper:

```toml
[[question]]
id = "gs2-governor-discretion"     # stable: the student's answers are kept under it
paper = "gs2"                      # gs1 to gs4
topics = ["mains.gs2.polity.federalism.challenges"]
source = "Civil Mentor sample"
marks = 15                         # 10 marks and 150 words, or 15 marks and 250 words
words = 250
question = "..."
points = ["...", "..."]            # what a good answer covers, ticked in self-evaluation
model = """
The model answer in Markdown, within the word limit.
"""
```

The loader refuses a question with other marks, a word limit that does not go with its marks, an unknown paper or no question, model answer or points, because the clock is set by the marks. It reports, and keeps, a question with an unknown topic or a model answer over its limit. Words are counted as an examiner would: every run of text with a letter or digit in it.

### What the store holds

| Table | One row per | Key fields |
|---|---|---|
| `attempt` | test sitting | kind, start, finish (empty while in progress), time allowed, marks for right and wrong |
| `answer` | question in a sitting | question ID, subject, topic, chosen option, correct, confidence, marked, time taken |
| `reading` | note or story opened | first, last and done dates, bookmark, seconds spent |
| `review` | flashcard or mistake | box (1 to 5), due date |
| `mark` | highlight or clip | item, text range, the student's comment |
| `my_note` | note the student wrote | title, body |
| `session` | block of study time | day, kind, item, seconds |
| `plan_task` | task in a day's plan | day, position, kind, target, title, minutes, done |
| `mains_answer` | written answer, drafts included | question, text, start and submit times, seconds on the clock, points ticked, seven self-scores |
| `setting` | setting | exam date, daily minutes, theme |

### Mentor rules, version 1

- **Today's plan** fills the daily minutes in this order: today's brief if unread, reviews due today (15 minutes at most), the note in progress or else the next unread note in the least-covered subject, a drill on the weakest topic that has enough attempts, and a Mains answer on alternate days. The plan is frozen for the day once made.
- **Personalised mix** samples questions by weight: previously wrong 2, unseen 1, previously right 0.2, and 1.5 times that for weak topics.
- **Review dates** use five boxes with gaps of 1, 3, 7, 16 and 35 days. Right moves a card up one box. Wrong sends it back to box 1.
- **Insights**: weakest topics (under 50% over at least 8 questions), marks won or lost by confidence level, seconds per question against the 72-second budget, attempts against net score, high-yield unread topics, and study time against the goal.

Each of these is a plain function with a unit test.

## 5. Build phases

Each phase ends with an app that runs and can be clicked through. The first build is Phases 0 to 4: the four pillars and the Today plan. Phases 5 and 6 follow.

| Phase | Goal | Status |
|---|---|---|
| 0. Shell | Window, navigation, content loader, store and search | Done |
| 1. Notes and syllabus | Read notes and track the whole syllabus | Done |
| 2. Practice | Timed tests in an exam room, with results and review | Done |
| 3. Analysis and Today | Charts, the mentor's rules and the daily plan | Done |
| 4. Current affairs | Daily briefs, the story reader and the daily quiz | Done |
| 5. Revision and notebook | Mistake book, flashcards, highlights and own notes | Done |
| 6. Mains | Answer writing against the clock, with self-evaluation | Done |

### How to work through a phase

Every phase below has a to-do list, so any agent can pick up the next open task.

- Read sections 1 to 4 first. The tasks refer to the layouts, store tables, content format and mentor rules defined there.
- Do the tasks in order. A task is done when its code, its sample content and its test are in, and `cargo test`, `cargo fmt --check` and `cargo clippy --all-targets` pass. Then tick its box.
- A phase is done when `cargo test` checks its "Done when" condition. Then add an "As built" note under the phase saying what differs from the plan, set its status in the table above, and update the status line at the top of this file.
- Every new table is a new numbered migration in `src/store.rs`. Never change an existing migration.
- Each new screen gets its own file in `src/` and replaces its stand-in in `src/screens.rs`. `tests/screens.rs` clicks through it and saves screenshots to `target/shots/`.
- Functions that depend on the date take today's date as an argument, so tests can move the clock.
- Content is our own, and sample content says so, as the current files do. Never copy notes, questions or text from deepmentor or anyone else. Real past questions come only from UPSC's published papers and answer keys.
- Commit straight to `main`, with no branches or pull requests, and only when the user asks.

### Phase 0: Shell

- [x] **0.1 Toolchain.** Update Rust to 1.100 and start the crate on `gpui-kit` 0.9.1, with `Cargo.lock` taken from the kit.
- [x] **0.2 Window shell.** Title bar with the exam countdown and a search box, a sidebar with ten sections that folds to an icon rail, the "Paper" light theme and a dark theme, shortcuts (⌘1 to ⌘5, ⌘, and ⌘\) and menus.
- [x] **0.3 Content loader.** Load `content/` (syllabus, notes, questions, daily briefs), link everything to the syllabus, and report mistakes in the files instead of dropping them.
- [x] **0.4 Store and settings.** One SQLite file in the app-data folder with numbered migrations, and a Settings screen for the exam date, daily study time and theme.
- [x] **0.5 Search.** A ⌘K palette over notes, stories and questions.
- [x] **0.6 Sample content and tests.** A starter content pack, stand-in screens for the later phases, unit tests for the loader and store, and an offscreen UI test that clicks through the shell and saves screenshots.

**Done when** the app launches, every sidebar item opens its screen, dummy content loads with no validation errors, and `cargo test` passes the loader and store round trip.

**As built.** All four conditions are met and checked by `cargo test`. What differs from the plan:

- **Content is a starter pack**, not the full dummy set: 83 syllabus topics, 4 notes, 30 questions and 2 daily briefs. Each later phase adds the rest of its own dummy data.
- **Mains questions and flashcards have no loader yet.** They arrive with Phases 6 and 5, when the screens that use them are designed.
- **Search opens the section a result belongs to**, not the item. Opening the note, question or story itself comes with Phases 1, 2 and 4, and so do the back and forward buttons.
- **The store has one table, `setting`.** The other tables are added by the phase that needs them, as numbered migrations.
- **The default Prelims date is 23 May 2027.** It is a placeholder for the student to change in Settings.

### Phase 1: Notes and syllabus

- [x] **1.1 Notes screen.** Notes listed by paper and subject, and a reader with Read and Distilled modes, an outline that jumps to sections, text size and width, and an "at a glance" line.
- [x] **1.2 Linked PYQs.** The questions on a note sit at its end, each with its answer behind a button.
- [x] **1.3 Mark as read and bookmark**, saved in the `reading` table.
- [x] **1.4 Full syllabus.** Prelims and Mains in `content/syllabus.toml`, split into subjects, chapters, topics and subtopics, with UPSC's wording on the matching nodes (see "Content format").
- [x] **1.5 Syllabus screen.** A folding tree with a status for every topic and coverage for every paper, subject and chapter. Clicking a topic opens its first unread note.
- [x] **1.6 Navigation.** Search opens the note itself, and back and forward work across screens and notes (also ⌘[ and ⌘]).
- [x] **1.7 Sample content and tests.** 12 notes (3 per GS paper), questions and stories tagged with the new syllabus ids, and UI test steps for the reader, mark as read, the syllabus tree and back and forward.

**Done when** a note can be read in both modes and marked read, and the coverage figure changes.

**As built.** The condition is met and checked by `cargo test`: the UI test opens a note, switches it to Distilled, marks it read and watches coverage go up for both Mains GS-2 and Prelims GS. What differs from the plan:

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

### Phase 2: Practice

- [x] **2.1 Store tables.** Add `attempt` and `answer`, with the fields under "What the store holds", as the next migration. Save each answer the moment it is given. Add store functions to start, answer, finish and resume an attempt.
- [x] **2.2 Scoring and question choice.** Start `src/mentor.rs`. `score()` gives +2 for right, minus a third of that for wrong and 0 for blank, reading the marks from the attempt so CSAT marking can be added later. `pick()` chooses the questions for each kind of test, with the personalised mix weighted as under "Mentor rules", and takes a seed so tests are repeatable. Unit-test the score against a hand-worked case.
- [x] **2.3 Practice hub.** Replace the stand-in with `src/practice.rs`: micro (10 questions), mini (30), full mock (100), subject drill (20), custom drill (the student picks subjects, count and time), personalised mix, past papers by year, and unfinished tests to resume. Unless the student sets the time, a test allows 72 seconds a question, as Prelims does. Grey out a choice the question bank cannot fill.
- [x] **2.4 Exam room.** `src/exam.rs`, taking the whole window as in the sketch under "Layout": one question at a time, a countdown, the question grid with answered, marked and not-visited counts, mark for review, clear, the confidence tag (Sure, 50-50, Guess) and option strike-out. Keys: A to D answer, M marks, the arrow keys move. The test submits itself when time runs out. After quitting or a crash it stays open with its remaining time, and Practice offers to resume it.
- [x] **2.5 Result and review.** The net score, right, wrong and blank counts, marks by subject and by confidence level, and time per question. Below that, every question with the chosen and correct answers, the explanation, the trap and a link to its note. `open(Target::Question)` now shows the question itself, so search results for questions land on it.
- [x] **2.6 Past papers.** Group questions whose `source` is "UPSC Prelims YYYY" by year. A paper can be read with its answers or sat as a full mock. A note's "at a glance" line gains the years its topics were asked in.
- [x] **2.7 Content and tests.** Grow the bank to about 100 original sample questions across the six subject files, tagged with Prelims nodes and linked to notes. Transcribe one real Prelims GS paper from upsc.gov.in with UPSC's answer key and our own explanations, and never put a UPSC label on an invented question. The UI test starts a drill, answers, quits, resumes, submits and reviews it, and saves screenshots.

**Done when** a drill can be sat against the clock, quit, resumed, submitted and reviewed, and a unit test checks the score against a hand-worked case.

**As built.** The condition is met and checked by `cargo test`. The UI test starts a Polity drill on a real data file, answers with the keys and the mouse, strikes out an option, tags confidence, marks a question, lets the clock run, leaves, resumes, then reopens the app on the same file as if it had crashed, resumes again with the time left, submits and reviews it. It also lets a micro test run out of time and submit itself, reads and sits the 2025 paper, and opens a question from search. `mentor::tests::the_score_matches_a_hand_worked_case` checks 6 right, 3 wrong and 1 blank at 10 marks. What differs from the plan:

- **Subjects for drills and marks are the seven a student thinks in**: History and culture, Geography, Polity, Economy, Environment, Science and technology, and Current affairs, each made of one or more Prelims syllabus subjects (`mentor::SUBJECTS`). The `answer` row stores that subject id and the question's first Prelims topic, so Phase 3 can group by either without the content.
- **The store keeps more than the table above lists.** `attempt` also has a title, the seconds used so far and the question on screen, so a test resumes where it was left. `answer` also has its position, the struck-out options and whether it was visited. Times are local and to the second.
- **Every change is saved at once**, and the clock is saved every second, so a crash loses at most a second. The clock only runs while the exam room is on screen; leaving stops it.
- **The exam room locks navigation.** Shortcuts, search and back do nothing there; only Save and leave, or Submit, lead out. Back after a test skips the exam room. On macOS the window's own buttons stay at the top left, so the header leaves room for them.
- **Submitting with blank or marked questions asks first**, in a bar under the header rather than a dialog.
- **Clear also clears the confidence tag**, and striking out the chosen option takes the answer back.
- **The personalised mix** uses the weights under "Mentor rules", with a topic weak below 50% over at least 8 answers, as for insights. The other random tests use the same weights without the weak-topic boost, so wrong answers come back in every test. It asks 20 questions.
- **The Practice hub also lists recent results**, so a result can be reviewed again. A test the bank cannot fill shows how many questions it has and its Start is greyed out (today only the Current affairs drill, with 12).
- **Result and review** sit in `src/review.rs`, which also shows one question on its own (where search and links land) and a past paper read with its answers. Time per question is the average over the whole test, and every question's own time is on its card.
- **Content**: 102 original sample questions (17 per subject file, 48 of them linked to notes), and the 2025 GS Paper I (Series A) with UPSC's key published on 13 May 2026, both from upsc.gov.in. No question was dropped from that key. Its explanations and traps are ours. Statement items keep UPSC's Roman numerals, so they match the options. A note's "at a glance" line now names the years its topics were asked.
- **A `dropped` field** was added to the question format for questions UPSC drops from a key (see "Content format").
- **Not built here, as planned**: the mistake book and `review` table (Phase 5), study-time `session` records and the Analysis and Today screens (Phase 3), the daily quiz (Phase 4) and CSAT marking. A new kind of test is a variant of `mentor::TestKind` with its pool.

### Phase 3: Analysis and Today

- [x] **3.1 Study time.** Add `session` and `plan_task` as migrations. Record the seconds spent reading and in tests, and add time spent to reading records (deferred from Phase 1). The streak is the number of days in a row, ending today or yesterday, with any study recorded.
- [x] **3.2 Mentor rules.** Add `today_plan()` and `insights()` to `src/mentor.rs`, exactly as under "Mentor rules", each with a unit test. The plan is saved to `plan_task` the first time it is made each day and stays frozen. Leave out the tasks whose screens come later (the brief, reviews and Mains answers) until their phase adds them.
- [x] **3.3 Today screen.** `src/today.rs`, following the sketch under "Layout": the date, days to the exam date in Settings, the streak, today's plan with tick boxes and tasks that open their target, continue reading, a question of the day, this week's study, and the mentor's insight cards with "Open note" and "Drill" buttons.
- [x] **3.4 Analysis screen.** `src/analysis.rs` with the tabs in the sketch: net score over the last 10 tests (line), accuracy by subject (bars), weakest topics with a Drill button, guess analysis (marks won or lost at each confidence level), study time over 12 weeks (heatmap), and syllabus read per paper from `syllabus::coverage`. Use the `gpui-kit` charts.
- [x] **3.5 Demo history.** A Settings action that adds 30 days of made-up attempts, answers, reading and study time, so Analysis is not empty, and one that removes them. Demo rows are marked, so removing them never touches real data.
- [x] **3.6 First-launch diagnostic.** The first time the app opens, offer a 20-question test across subjects that seeds the weak areas (addition 4 under "Additions I recommend").
- [x] **3.7 Tests.** The UI test loads demo history and checks that the charts draw and that the plan and insights appear, with screenshots.

**Done when** with demo history loaded the charts draw, and the plan and insights appear and are covered by tests.

**As built.** The condition is met and checked by `cargo test`. The UI test takes and declines the diagnostic, reads a note for 60 seconds and then past the idle limit, adds demo history from Settings, checks that every chart and the heatmap draw coloured pixels and that the plan, the insight cards and the guess analysis appear, ticks a task and opens both tasks, sits a drill from Analysis, then removes the demo history and finds every real row as it was. `mentor::tests::insights_match_a_hand_worked_record` and the three plan tests work their numbers by hand, and `store::tests::removing_demo_history_never_touches_a_real_row` checks the store side. What differs from the plan:

- **Weak topics are counted by chapter** (`pre.gs.polity.parliament`), not by the answer's own topic, which is often a subtopic with too few answers to reach 8. The personalised mix from Phase 2 uses the same chapters.
- **Two new kinds of test**, as variants of `mentor::TestKind`: `Topic` drills one chapter with 10 questions, or the whole pool when the bank has fewer (the plan's drill, the Drill buttons and the mentor cards use it), and `Diagnostic` asks 20 questions spread across the seven subjects in 24 minutes.
- **Today's plan has two kinds of task for now**: the note in progress or else the next unread note, then a drill on the weakest chapter with at least 8 answers. The brief, reviews and Mains answers are left out until their phases, so the plan does not yet fill the daily minutes. A note task is the note's minutes less the time already spent, at least 5 minutes. A drill fits the time left at 72 seconds a question and needs at least 5 questions, or the whole pool when it is smaller. The plan is saved to `plan_task` the first time Today opens each day. A task ticks itself when its note is marked read or its drill is finished that day, and can be ticked by hand.
- **Study time** comes from two places. The reader counts a second at a time while a note is on screen, stops counting after 5 minutes with no scrolling or mouse movement, and saves every 15 seconds into `reading.seconds` and a `session` row. The exam room adds the seconds a test uses. The streak counts days with any seconds.
- **Demo rows are marked by a `demo` column** on `attempt`, `reading` and `session`, added by migration 5; migration 4 adds `session`, `plan_task` and `reading.seconds`. Demo history is 24 finished tests over the 30 days ending yesterday with 6 rest days, plus reading and study time, built from the real content with a fixed seed and weak on Parliament. A note the student opens themselves stops being a demo row. Adding or removing demo history makes today's plan again unless a task in it is already ticked. Analysis shows a banner while demo rows are present.
- **The diagnostic is offered on Today** until it is taken or declined, and only while the student has no test of their own (demo tests do not count). Either answer is saved as a setting, so it is offered once.
- **Analysis has more than the sketch.** Overview is the sketch. Subjects adds a table per subject, Tests adds attempts against net score and the recent tests, Time adds hours a week and seconds per question against the 72-second budget, and Coverage adds high-yield unread topics. The line and bar charts are `gpui-kit`'s; the 12-week heatmap is plain cells, as the kit has no heatmap.
- **Today's mentor shows the first 3 insight cards.** "Open note" appears only when a note covers the card's topic. The title bar shows the countdown and the streak. The question of the day is chosen by the date, with its answer behind a button; it is not recorded as a test.

### Phase 4: Current affairs

- [x] **4.1 Brief format and content.** Give every story a stable `id` so its reading can be recorded, and add editorials to a day's file: our own analysis of an issue in the news, never newspaper text. Grow the sample to 7 days of briefs with 5 stories each, every story with a practice question.
- [x] **4.2 Current affairs screen.** `src/current_affairs.rs` with the panes in the sketch: the date with previous, next and a calendar; the stories grouped by subject, then the day's editorials; and the story reader (context, why it matters, key facts, the trap, PYQ pattern, the practice question with its answer inline, the Mains question with its answer frame, and linked notes). Rapid mode shows only each story's title and key facts. `open(Target::Story)` now shows the story itself.
- [x] **4.3 Reading and bookmarks.** Stories get `reading` rows and bookmarks the way notes do, and the date bar shows how many of the day's stories are read. Today's plan gains its brief task, ticked once all the day's stories are read.
- [x] **4.4 Daily quiz.** "Take today's quiz" sits the day's practice questions in the exam room as their own kind of test, and the result counts in Analysis.
- [x] **4.5 Monthly issue.** One page per month with all its stories grouped by subject, and a quiz over that month's questions.
- [x] **4.6 Tests.** The UI test reads a brief in both modes, takes its quiz, and checks that Today and Analysis show both, with screenshots.

**Done when** a brief can be read and its quiz taken, and both show up in Today and Analysis.

**As built.** The condition is met and checked by `cargo test`. The UI test follows the brief task from Today to the day's brief, moves a day back and forward and picks a day on the calendar, opens a story (every part shows, it counts as read, 30 seconds of reading count as study time) and bookmarks it, goes back and forward between stories, opens an editorial, marks the day read in Rapid mode, then finds the brief task done, sits the day's quiz to a result, and finds its score on Today, on the brief and in Analysis → Tests, then opens the monthly issue and starts its 35-question quiz. `mentor::tests::the_brief_comes_first_until_every_story_is_read` and `a_quiz_asks_its_briefs_questions_in_order` check the rules. What differs from the plan:

- **Which brief is today's.** The newest brief dated on or before today, or the oldest when every brief is later. The sample covers 10 to 16 Nov 2026, so on any later day the 16 Nov brief is "today's" until real briefs arrive, and the plan task then reads "Read the brief for 16 Nov". The date bar opens on it.
- **Reading is stored in the existing `reading` table**, under the keys `story:<id>` and `editorial:<id>`, so no migration was needed. Opening a story counts it read. Rapid mode does not, but ends with "Mark them all read". Reading time counts as a `session` of kind `story`, so it adds to study time and the streak; the notes reader's clock was moved into a shared `ReadingClock` for this.
- **The brief task** comes first in the plan, 5 minutes for each unread story (25 for a new brief), and ticks itself once all the brief's stories are read. Its row on Today also has "Take its quiz", and once the quiz is sat it shows the score. When every story is read the plan leaves the brief out, as the mentor rules say.
- **Two new kinds of test**: `Quiz(date)` asks the brief's 5 practice questions in story order, and `Month(year, month)` asks every practice question of the month, oldest day first, both at 72 seconds a question with Prelims marking. They count in the analysis like any test. **Quiz results show in Analysis → Tests** in their own "Current affairs" panel, with stories read, every quiz with its score, and the share right; they also appear in Recent tests and the charts.
- **The calendar is plain cells**: each month that has briefs, with those days shaded and clickable. gpui-kit's date picker needs a popover the offscreen test cannot drive reliably.
- **Stories are read in the list's order**, grouped by subject in syllabus order: the brief's overview, Next, "Read the next story" and Rapid mode all follow it. The day's quiz keeps the file's story order.
- **The monthly issue** is a view of the Current affairs screen, opened by the "November issue" button in the date bar: the month's stories by subject, each opening its story, and the monthly quiz.
- **Bookmarks** use the `bookmarked` column of the story's reading row. The Notebook (Phase 5) is where they will be listed; for now a story shows Save or Saved.
- **Search** finds editorials as well as stories.
- **Two Phase 3 fixes**: the question of the day renders its stem as Markdown, so a table no longer shows as raw pipes, and the attempts card on Today only advises attempting more when some questions were left blank. When every question was attempted at a positive return, it says attempting pays.
- **Content**: 7 briefs of 5 stories and 1 editorial each, all original and marked as sample. The stories are illustrative events. Their key facts, traps and practice questions are static facts. Every story's were read against the record, and the ones that are not standard textbook facts were checked by web search. One wrong fact (the Semi-Conductor Laboratory's ministry) was corrected, and three loose ones were tightened: where Kharosthi was used, Chhau's place on the two lists of classical dances, and Bt cotton as the only GM crop grown commercially.

### Phase 5: Revision and notebook

- [x] **5.1 Review dates and the mistake book.** Add `review` as a migration. `mentor::review()` moves an item through five boxes with gaps of 1, 3, 7, 16 and 35 days: right moves it up one box, wrong sends it back to box 1, and right in box 5 retires it. Every wrong answer in a test joins the mistake book, due the next day. Unit-test the dates.
- [x] **5.2 Flashcards.** A loader for `content/flashcards/` (front, back, topics, linked note), validated like questions, and 30 original sample cards.
- [x] **5.3 Review session.** `src/revision.rs`: the mistake book by subject, and a session with today's due items, where mistakes are asked again as questions and flashcards are flipped and marked "Got it" or "Missed". Each answer updates its box. Today's plan gains its review task (15 minutes at most), which opens the session.
- [x] **5.4 Highlights and clips.** Select text in the note or story reader to highlight it, with an optional comment, or to save it as a clip. Both go in `mark` (item, text range, comment) and show again when the item is reopened.
- [x] **5.5 Notebook screen.** `src/notebook.rs`: highlights, clips and bookmarks, each opening its source, and the student's own notes (`my_note`: a title and a Markdown body), which search also finds.
- [x] **5.6 Focus timer.** A 25-minute timer at the foot of the sidebar that records a focus `session` when it finishes.
- [x] **5.7 Tests.** The UI test answers a question wrong, moves the date to its due day and finds it in the review session, then makes a highlight, reopens the app on the same store and finds it again.

**Done when** a wrong answer comes back on its due date, and a highlight survives a restart.

**As built.** The condition is met and checked by `cargo test`. The UI test answers a question wrong in a short test, finds it in the mistake book, moves the date to its due day and finds the review task on Today, sits the session and watches the mistake move from box 1 to box 2, due three days on, then flips a flashcard and marks it. A second test highlights a passage of a note with a comment, clips another, restarts the app on the same file and finds both marks still there and the highlight painted on the note again, highlights a story, then walks the Notebook, saves a note of its own, finds that note with ⌘K and runs a focus block through its 25 minutes. `mentor::tests` checks the box gaps and the plan's review task against hand-worked cases, and `store::tests` checks the two new tables, the upgrade from a Phase 5.1 file and that removing demo history leaves marks and own notes alone. What differs from the plan:

- **The backfill of mistakes made before this phase.** An installation that has been used already holds wrong answers with no box. They join the book once, at start-up, ten a day starting the day after, taken from real tests only and never from the demo history, so the first session does not open on a hundred questions at once. The day it ran is kept in `setting`, so it never runs again. `mentor::BACKFILL_PER_DAY` sets the spread.
- **A re-asked mistake is study time, not a test attempt.** Answering a mistake again or marking a flashcard records a `session` row of kind `review` with the item `session`, and nothing in `attempt` or `answer`. Analysis therefore keeps reporting only real sittings, and the minutes still count towards the day, the streak and the study-time charts. Time between two answers is capped at the same five idle minutes the readers use, so a session left open over lunch does not count.
- **Highlights are true character ranges.** `TextViewState` records the source position of every character it renders, paints `RangeHighlight`s behind the glyphs without moving the layout, and reports a selection as a source range, which the offscreen test drives with a real pointer drag. So a highlight covers exactly the words selected, and it is painted on the note or story itself rather than in a list beside it. Each mark also stores the words it covered: when the item is reopened the stored range is checked against the text, and if the content has been edited the words are looked for near where they were, so a small edit does not lose a highlight. A mark whose words have gone stops painting but stays in the Notebook.
- **A `part` column on `mark`.** A note is read as its sections and a story as its named parts (context, why it matters, key facts, the trap, PYQ pattern, the Mains question), each its own text view, so a mark is filed under the part it was made in and its range is measured in that part's text. Reopening in Distilled mode shows the Read mode's marks only where the words are the same.
- **The story reader shows its prose as Markdown.** Context, Why it matters, Key facts, the trap, the PYQ pattern and the Mains question were one-line divs; they are Markdown text views now, with the key facts as a bullet list, so they can be selected and highlighted like a note. The headings, byline, practice question and answer frame are unchanged.
- **A selection outlives the click that marks it.** A press clears the window's text selection before a button's click runs, so both readers watch their text views and keep the last selection; Highlight and Clip act on that.
- **The Notebook** replaces its stand-in. Its filters are All, Highlights, Clips, Bookmarks and My notes. A mark shows the words, the comment and where it came from, and opens the source; a bookmark opens its note, story or editorial; a note of the student's own is written in the pane and saved with a button, and ⌘K searches its title and body and opens it. Bookmarks are still the `reading` row's flag, as in Phase 4, so a note or story bookmarked in its reader appears here without a second table.
- **The focus timer** sits above Settings at the foot of the sidebar and counts 25:00 down a second at a time. When it finishes it records a `focus` session with only the minutes no other session counted during the block, so a note read while the timer ran is not counted twice. Stopping early records nothing.
- **Content**: 30 cards, 6 polity, 6 history, 5 geography, 5 economy, 4 environment and 4 science and technology, one fact each, all original and marked as sample, each linked to a note that carries its fact. Every fact is a standard textbook one; the two that could drift (the inflation target and the number of WPA schedules) were checked against the record.
- **Not built here, as planned**: decks the student makes, card editing, import and export, sync, cheatsheets as a new note kind, and the Mains work of Phase 6.

### Phase 6: Mains

- [x] **6.1 Mains questions.** A loader for `content/mains/` (id, paper, topics, source, marks, word limit, the question, a model answer in Markdown, and the points to cover), validated like questions, and 10 original sample questions with model answers.
- [x] **6.2 Mains screen.** `src/mains.rs`: questions by paper and topic, filtered by paper and by answered or not. The model answer stays hidden until the student has written an answer or asks to see it.
- [x] **6.3 Timed editor.** A countdown set by the marks (7 minutes for 10 marks, 11 for 15) and a live word count against the limit (150 or 250 words). Save the answer to `mains_answer` as the student types, so a crash loses nothing.
- [x] **6.4 Self-evaluation.** After submitting, the student sees the model answer and the points to cover beside their own answer, ticks the points they made, and scores themselves from 0 to 5 on seven dimensions: directive (does the answer do what the question asks), introduction, content, dimensions covered, examples and data, conclusion, and presentation. deepmentor's site names only its first and last dimensions, directive and presentation, so the five in between are our choice.
- [x] **6.5 History.** Every answer with its date, time taken, word count and scores, and the trend per dimension. Today's plan gains its Mains task on alternate days, which opens an unanswered question.
- [x] **6.6 Tests.** The UI test writes an answer, scores it and finds it in the history, with screenshots.

**Done when** an answer can be written, scored and seen in the history.

**As built.** The condition is met and checked by `cargo test`. The UI test opens today's plan, finds the Mains task last in it and opens its question with the model answer hidden, filters the list by paper and by answered, then writes an answer to a 15-mark question while its 11 minutes count down. It closes the app mid-answer, reopens it on the same file and finds the words and the clock where they were, lets the time run out, writes on into the overrun, submits, ticks points and gives seven scores, and finds the answer in the history. It then moves the date on a day (no Mains task) and two days (the task is back, from GS-1, the paper with no answer yet), asks for a model answer before writing, writes and scores a second answer and proves the seven trend charts drew their lines by their coloured pixels. `mentor::tests` checks the clock (7 minutes for 10 marks, 11 for 15, and the overrun), the alternate-day task, when it is done and the trend. `store::tests` checks that an answer is saved as typed and survives a crash, that a Phase 5 file upgrades with every row of every table intact, and that removing demo history leaves the student's Mains answers and drafts alone. `content::tests` checks that the 10 questions load cleanly and that bad ones are reported. What differs from the plan, and the calls this phase made:

- **Alternate days count from the student's own writing, not the calendar.** The plan has no Mains task on the day after one on which an answer was submitted, and has one on every other day. So a day missed brings the task back the next day instead of two days later. The task finishes the latest draft if there is one, else writes an unanswered question from the paper with the fewest answers, else rewrites the answer written longest ago, so the task does not stop once all 10 are answered. It takes the question's clock and 5 minutes to score it: 16 minutes for 15 marks, the sketch's figure, and 12 for 10. It is done once any answer is submitted that day.
- **A draft resumes after leaving or a crash.** The answer is saved the moment writing starts, then on every keystroke and every second of its clock, so a crash loses at most a second. The clock runs only while the editor is on screen, and stands where it stopped. At zero the editor stays open: the clock shows the overrun in red, a banner says time is up, and the overrun is kept with the answer and shown in the history. Once time is up the clock counts only while the student types (the readers' five idle minutes), so an editor left open does not run for hours.
- **Writing time is study time**: a `session` row of a new kind, `mains`, with the answer's id as its item, so it counts towards the day, the streak and the charts.
- **Demo history writes no Mains answers.** A made-up answer would need made-up text, and would mark questions as answered that the student never wrote, which would also steer the Mains task away from them. `mains_answer` therefore has no demo column, and removing demo history never touches it.
- **The loader handles marks strictly**: 10 marks with 150 words or 15 with 250, nothing else, as under "Content format".
- **The Mains screen** has two tabs. Questions groups them by paper and then by subject, with filters for the paper and for answered or not, and each row shows its marks, limit, clock and whether it is unanswered, a draft or answered with its last score. A question's page offers Start writing (or Resume your draft, or Write it again) and Show the model answer, which is hidden until the student has submitted an answer or asks for it. The editor shows the question, the clock, the word count against the limit (red when over), Save and leave, and Submit and score it. History lists every submitted answer with its date, time taken (overrun in red), word count, points made and total out of 35, under a small chart per dimension with its average and its change (the last up to three scores against as many before them); each entry opens its scoring sheet again, where ticks and scores can be changed.
- **The seven dimensions** are directive, introduction, content, dimensions covered, examples and data, conclusion and presentation, each with the question the student asks of their own answer. Scores and ticks are saved as they are clicked; the total shows once all seven are given.
- **The stand-in screens are gone.** Mains replaced the last one, so `src/screens.rs` keeps only the page column and counted nouns.
- **A test helper, `centre`.** `reveal` stops once any part of an element shows, so a click at its centre can miss below the edge of the window, which happened on the scoring sheet. `centre` scrolls the element towards the middle of the window first.
- **Content**: 10 original questions, all marked as sample: three from GS-1 (the Western Ghats and rainfall, the Non-Cooperation Movement, urbanisation and the joint family), three from GS-2 (the Governor's discretion, self-help groups, the Indian Ocean), three from GS-3 (the Monetary Policy Committee, micro-irrigation, heatwaves) and one from GS-4 (probity), five of 10 marks and five of 15. No 20-mark GS-4 case study, as planned. Facts that are not textbook staples were checked on the web: IMD's heatwave thresholds, Ahmedabad's 2013 heat action plan as South Asia's first, NABARD's description of the SHG–Bank Linkage Programme as the world's largest microfinance programme, the fourteen-day rule for publishing MPC minutes, and the notification of heatwaves as a calamity for the disaster response funds on 4 August 2026. The flashcard on the inflation target was checked too: the notification of 25 March 2026 kept 4% with a band of 2% to 6% for April 2026 to March 2031.
- **Not built here, as planned**: AI evaluation, the Essay paper, GS-4 case studies, handwritten or uploaded answers, sync, and links from the current-affairs stories' Mains questions to this screen.

### Later

In no fixed order: AI chat and AI answer evaluation, a content sync server, accounts and plans, atlas, optionals, CSAT lessons, Hindi, Windows and Linux builds.

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
