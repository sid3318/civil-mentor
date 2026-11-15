//! Everything the student does, kept in one SQLite file in the OS app-data folder.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use chrono::{NaiveDate, NaiveDateTime};
use rusqlite::{Connection, OptionalExtension as _, params};

/// Schema changes, oldest first. Never edit an entry that has shipped: add a new
/// one. `PRAGMA user_version` records how many have been applied to a file.
const MIGRATIONS: &[&str] = &[
    "CREATE TABLE setting (key TEXT PRIMARY KEY, value TEXT NOT NULL) STRICT",
    // One row per note the student has opened. Dates are YYYY-MM-DD.
    "CREATE TABLE reading (
        item TEXT PRIMARY KEY,
        first TEXT NOT NULL,
        last TEXT NOT NULL,
        done TEXT,
        bookmarked INTEGER NOT NULL DEFAULT 0
    ) STRICT",
    // One row per test sitting, and one per question in it. Answers are
    // saved as they are given, so an unfinished sitting can be resumed.
    // Times are local, YYYY-MM-DD HH:MM:SS; `finished` is empty until it is
    // submitted. Marks are kept per sitting so other markings can be added.
    "CREATE TABLE attempt (
        id INTEGER PRIMARY KEY,
        kind TEXT NOT NULL,
        title TEXT NOT NULL,
        started TEXT NOT NULL,
        finished TEXT,
        seconds_allowed INTEGER NOT NULL,
        seconds_used INTEGER NOT NULL DEFAULT 0,
        right_marks REAL NOT NULL,
        wrong_marks REAL NOT NULL,
        current INTEGER NOT NULL DEFAULT 0
    ) STRICT;
    CREATE TABLE answer (
        attempt INTEGER NOT NULL REFERENCES attempt (id) ON DELETE CASCADE,
        position INTEGER NOT NULL,
        question TEXT NOT NULL,
        subject TEXT NOT NULL,
        topic TEXT NOT NULL,
        chosen INTEGER,
        correct INTEGER,
        confidence TEXT,
        marked INTEGER NOT NULL DEFAULT 0,
        struck INTEGER NOT NULL DEFAULT 0,
        visited INTEGER NOT NULL DEFAULT 0,
        seconds INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (attempt, position)
    ) STRICT",
];

const TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

// ponytail: queries run on the UI thread. They take microseconds at one
// student's volume; move them to the background executor if one ever shows up
// in a frame profile.
pub struct Store {
    conn: Connection,
    path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeChoice {
    Light,
    Dark,
    System,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 3] = [ThemeChoice::Light, ThemeChoice::Dark, ThemeChoice::System];

    pub fn name(self) -> &'static str {
        match self {
            ThemeChoice::Light => "Light",
            ThemeChoice::Dark => "Dark",
            ThemeChoice::System => "System",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The day of the Prelims paper the student is preparing for.
    pub exam_date: NaiveDate,
    /// How long the student means to study each day. Today's plan is sized to it.
    pub daily_minutes: u32,
    pub theme: ThemeChoice,
    /// Body text size in the reader, in pixels.
    pub reader_size: u32,
    /// Whether the reader uses the wide column.
    pub reader_wide: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            // A placeholder until the student sets their own date in Settings.
            exam_date: NaiveDate::from_ymd_opt(2027, 5, 23).expect("valid date"),
            daily_minutes: 90,
            theme: ThemeChoice::Light,
            reader_size: 16,
            reader_wide: false,
        }
    }
}

/// What the student has done with one note. A note with no `Reading` has
/// never been opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reading {
    /// The day it was first opened.
    pub first: NaiveDate,
    /// The day it was last opened.
    pub last: NaiveDate,
    /// The day the student marked it read.
    pub done: Option<NaiveDate>,
    pub bookmarked: bool,
}

impl Reading {
    /// A note opened for the first time today.
    pub fn new(today: NaiveDate) -> Reading {
        Reading {
            first: today,
            last: today,
            done: None,
            bookmarked: false,
        }
    }
}

/// How sure the student was of an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Confidence {
    Sure,
    FiftyFifty,
    Guess,
}

impl Confidence {
    pub const ALL: [Confidence; 3] = [Confidence::Sure, Confidence::FiftyFifty, Confidence::Guess];

    pub fn label(self) -> &'static str {
        match self {
            Confidence::Sure => "Sure",
            Confidence::FiftyFifty => "50-50",
            Confidence::Guess => "Guess",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Confidence::Sure => "sure",
            Confidence::FiftyFifty => "fifty-fifty",
            Confidence::Guess => "guess",
        }
    }
}

/// One test sitting: its questions in order and what the student did with each.
#[derive(Debug, Clone, PartialEq)]
pub struct Attempt {
    /// Set by the store when the sitting is started.
    pub id: i64,
    /// What kind of test it is, as [`crate::mentor::TestKind::key`] writes it.
    pub kind: String,
    pub title: String,
    pub started: NaiveDateTime,
    /// `None` while the test is still open.
    pub finished: Option<NaiveDateTime>,
    pub seconds_allowed: u32,
    /// Time on the clock so far. The test has `seconds_allowed - seconds_used` left.
    pub seconds_used: u32,
    pub right_marks: f64,
    /// Taken off for a wrong answer, as a positive number.
    pub wrong_marks: f64,
    /// The question on screen when the student last left, by position.
    pub current: usize,
    pub answers: Vec<Answer>,
}

impl Attempt {
    pub fn seconds_left(&self) -> u32 {
        self.seconds_allowed.saturating_sub(self.seconds_used)
    }

    pub fn answered(&self) -> usize {
        self.answers.iter().filter(|a| a.chosen.is_some()).count()
    }
}

/// One question in a sitting. `subject` and `topic` are copied from the
/// question when the test starts, so analysis does not need the content.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Answer {
    pub question: String,
    pub subject: String,
    pub topic: String,
    /// The option chosen, 0 to 3, or `None` while blank.
    pub chosen: Option<usize>,
    /// `None` while blank, and for a question UPSC dropped from its key.
    pub correct: Option<bool>,
    pub confidence: Option<Confidence>,
    /// Marked for review.
    pub marked: bool,
    /// Options struck out, one bit per option.
    pub struck: u8,
    pub visited: bool,
    /// Time spent on this question.
    pub seconds: u32,
}

impl Answer {
    pub fn is_struck(&self, option: usize) -> bool {
        self.struck & (1 << option) != 0
    }

    pub fn toggle_strike(&mut self, option: usize) {
        self.struck ^= 1 << option;
    }
}

/// An answered question from any sitting, for choosing what to ask next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Past {
    pub question: String,
    pub topic: String,
    pub correct: bool,
}

/// Where the student's data lives: `~/Library/Application Support/CivilMentor`
/// on macOS, `%APPDATA%\CivilMentor` on Windows, `~/.local/share/CivilMentor` on Linux.
pub fn default_path() -> Result<PathBuf> {
    let dir = dirs::data_dir().context("this system has no app-data folder")?;
    Ok(dir.join("CivilMentor").join("civil-mentor.db"))
}

impl Store {
    pub fn open(path: &Path) -> Result<Store> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("could not create {}", dir.display()))?;
        }
        let conn =
            Connection::open(path).with_context(|| format!("could not open {}", path.display()))?;
        Store::prepare(conn, Some(path.to_path_buf()))
    }

    /// A store that vanishes when the app closes. Used by tests, and as the
    /// fallback when the real file cannot be opened.
    pub fn in_memory() -> Store {
        let conn = Connection::open_in_memory().expect("SQLite can open an in-memory database");
        Store::prepare(conn, None).expect("the schema applies to an empty database")
    }

    fn prepare(conn: Connection, path: Option<PathBuf>) -> Result<Store> {
        // WAL keeps the file consistent if the app is killed mid-write.
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
        let applied: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let applied = applied as usize;
        if applied > MIGRATIONS.len() {
            // Opening it anyway could write old-shaped rows into a newer schema.
            bail!("the data file was written by a newer version of Civil Mentor");
        }
        for (ix, sql) in MIGRATIONS.iter().enumerate().skip(applied) {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (ix + 1) as u32)?;
            tx.commit()?;
        }
        Ok(Store { conn, path })
    }

    /// The file behind this store, or `None` for an in-memory store.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    fn get(&self, key: &str) -> Result<Option<String>> {
        let value = self
            .conn
            .query_row("SELECT value FROM setting WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?;
        Ok(value)
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO setting (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// The saved settings. A value that is missing or unreadable falls back to
    /// its default, so one bad row cannot stop the app from starting.
    pub fn settings(&self) -> Settings {
        let read = |key: &str| self.get(key).ok().flatten();
        let defaults = Settings::default();
        Settings {
            exam_date: read("exam_date")
                .and_then(|value| value.parse().ok())
                .unwrap_or(defaults.exam_date),
            daily_minutes: read("daily_minutes")
                .and_then(|value| value.parse().ok())
                .unwrap_or(defaults.daily_minutes),
            theme: read("theme")
                .and_then(|value| ThemeChoice::ALL.into_iter().find(|t| t.name() == value))
                .unwrap_or(defaults.theme),
            reader_size: read("reader_size")
                .and_then(|value| value.parse().ok())
                .unwrap_or(defaults.reader_size),
            reader_wide: read("reader_wide")
                .and_then(|value| value.parse().ok())
                .unwrap_or(defaults.reader_wide),
        }
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        self.set("exam_date", &settings.exam_date.to_string())?;
        self.set("daily_minutes", &settings.daily_minutes.to_string())?;
        self.set("theme", settings.theme.name())?;
        self.set("reader_size", &settings.reader_size.to_string())?;
        self.set("reader_wide", &settings.reader_wide.to_string())?;
        Ok(())
    }

    /// Every note the student has opened, by note id. A row whose dates
    /// cannot be read is left out rather than stopping the rest loading.
    pub fn readings(&self) -> Result<HashMap<String, Reading>> {
        let mut query = self
            .conn
            .prepare("SELECT item, first, last, done, bookmarked FROM reading")?;
        let rows = query.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, bool>(4)?,
            ))
        })?;
        let mut readings = HashMap::new();
        for row in rows {
            let (item, first, last, done, bookmarked) = row?;
            let (Ok(first), Ok(last)) = (first.parse(), last.parse()) else {
                continue;
            };
            let done = done.and_then(|done| done.parse().ok());
            readings.insert(
                item,
                Reading {
                    first,
                    last,
                    done,
                    bookmarked,
                },
            );
        }
        Ok(readings)
    }

    pub fn save_reading(&self, item: &str, reading: &Reading) -> Result<()> {
        self.conn.execute(
            "INSERT INTO reading (item, first, last, done, bookmarked) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (item) DO UPDATE SET first = excluded.first, last = excluded.last,
                 done = excluded.done, bookmarked = excluded.bookmarked",
            params![
                item,
                reading.first.to_string(),
                reading.last.to_string(),
                reading.done.map(|done| done.to_string()),
                reading.bookmarked
            ],
        )?;
        Ok(())
    }
}

// ---- tests and answers -----------------------------------------------------

impl Store {
    /// Saves a new sitting with all its questions and returns its id.
    pub fn start_attempt(&self, attempt: &Attempt) -> Result<i64> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO attempt (kind, title, started, finished, seconds_allowed, seconds_used,
                 right_marks, wrong_marks, current)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                attempt.kind,
                attempt.title,
                attempt.started.format(TIME_FORMAT).to_string(),
                attempt.finished.map(|t| t.format(TIME_FORMAT).to_string()),
                attempt.seconds_allowed,
                attempt.seconds_used,
                attempt.right_marks,
                attempt.wrong_marks,
                attempt.current as i64,
            ],
        )?;
        let id = tx.last_insert_rowid();
        for (position, answer) in attempt.answers.iter().enumerate() {
            tx.execute(
                "INSERT INTO answer (attempt, position, question, subject, topic)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    id,
                    position as i64,
                    answer.question,
                    answer.subject,
                    answer.topic
                ],
            )?;
            write_answer(&tx, id, position, answer)?;
        }
        tx.commit()?;
        Ok(id)
    }

    /// Saves one answer as it stands, with the clock and the question on
    /// screen, in one transaction. Called on every change and every tick,
    /// so a crash loses at most a second.
    pub fn save_answer(&self, attempt: &Attempt, position: usize) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE attempt SET seconds_used = ?2, current = ?3 WHERE id = ?1",
            params![attempt.id, attempt.seconds_used, attempt.current as i64],
        )?;
        if let Some(answer) = attempt.answers.get(position) {
            write_answer(&tx, attempt.id, position, answer)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Closes a sitting that has `finished` set.
    pub fn finish_attempt(&self, attempt: &Attempt) -> Result<()> {
        let Some(finished) = attempt.finished else {
            bail!("the test has not been submitted");
        };
        self.conn.execute(
            "UPDATE attempt SET finished = ?2, seconds_used = ?3 WHERE id = ?1",
            params![
                attempt.id,
                finished.format(TIME_FORMAT).to_string(),
                attempt.seconds_used
            ],
        )?;
        Ok(())
    }

    /// One sitting with its answers, to resume it or show its result.
    pub fn attempt(&self, id: i64) -> Result<Option<Attempt>> {
        Ok(self.attempts_where("id = ?1", params![id])?.pop())
    }

    /// Sittings that were started and never submitted, newest first.
    pub fn unfinished(&self) -> Result<Vec<Attempt>> {
        self.attempts_where("finished IS NULL", [])
    }

    /// The most recent submitted sittings, newest first.
    pub fn finished(&self, limit: usize) -> Result<Vec<Attempt>> {
        self.attempts_where(
            "finished IS NOT NULL ORDER BY finished DESC, id DESC LIMIT ?1",
            params![limit as i64],
        )
    }

    fn attempts_where(&self, filter: &str, args: impl rusqlite::Params) -> Result<Vec<Attempt>> {
        let order = if filter.contains("ORDER BY") {
            ""
        } else {
            " ORDER BY id DESC"
        };
        let mut query = self.conn.prepare(&format!(
            "SELECT id, kind, title, started, finished, seconds_allowed, seconds_used,
                 right_marks, wrong_marks, current
             FROM attempt WHERE {filter}{order}"
        ))?;
        let rows = query.query_map(args, |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, u32>(5)?,
                row.get::<_, u32>(6)?,
                row.get::<_, f64>(7)?,
                row.get::<_, f64>(8)?,
                row.get::<_, i64>(9)?,
            ))
        })?;
        let mut attempts = Vec::new();
        for row in rows {
            let (id, kind, title, started, finished, allowed, used, right, wrong, current) = row?;
            let started = NaiveDateTime::parse_from_str(&started, TIME_FORMAT)
                .with_context(|| format!("test {id} has an unreadable start time"))?;
            let finished = match finished {
                Some(text) => Some(
                    NaiveDateTime::parse_from_str(&text, TIME_FORMAT)
                        .with_context(|| format!("test {id} has an unreadable finish time"))?,
                ),
                None => None,
            };
            attempts.push(Attempt {
                id,
                kind,
                title,
                started,
                finished,
                seconds_allowed: allowed,
                seconds_used: used,
                right_marks: right,
                wrong_marks: wrong,
                current: current.max(0) as usize,
                answers: self.answers(id)?,
            });
        }
        Ok(attempts)
    }

    fn answers(&self, attempt: i64) -> Result<Vec<Answer>> {
        let mut query = self.conn.prepare(
            "SELECT question, subject, topic, chosen, correct, confidence, marked, struck,
                 visited, seconds
             FROM answer WHERE attempt = ?1 ORDER BY position",
        )?;
        let rows = query.query_map([attempt], |row| {
            let confidence: Option<String> = row.get(5)?;
            Ok(Answer {
                question: row.get(0)?,
                subject: row.get(1)?,
                topic: row.get(2)?,
                chosen: row.get::<_, Option<i64>>(3)?.map(|c| c as usize),
                correct: row.get(4)?,
                confidence: confidence
                    .and_then(|key| Confidence::ALL.into_iter().find(|c| c.key() == key)),
                marked: row.get(6)?,
                struck: row.get(7)?,
                visited: row.get(8)?,
                seconds: row.get(9)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Every answered question with a right or wrong outcome, oldest first.
    pub fn answered(&self) -> Result<Vec<Past>> {
        let mut query = self.conn.prepare(
            "SELECT question, topic, correct FROM answer
             WHERE correct IS NOT NULL ORDER BY attempt, position",
        )?;
        let rows = query.query_map([], |row| {
            Ok(Past {
                question: row.get(0)?,
                topic: row.get(1)?,
                correct: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

fn write_answer(conn: &Connection, attempt: i64, position: usize, answer: &Answer) -> Result<()> {
    conn.execute(
        "UPDATE answer SET chosen = ?3, correct = ?4, confidence = ?5, marked = ?6, struck = ?7,
             visited = ?8, seconds = ?9
         WHERE attempt = ?1 AND position = ?2",
        params![
            attempt,
            position as i64,
            answer.chosen.map(|c| c as i64),
            answer.correct,
            answer.confidence.map(Confidence::key),
            answer.marked,
            answer.struck,
            answer.visited,
            answer.seconds,
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn changed() -> Settings {
        Settings {
            exam_date: NaiveDate::from_ymd_opt(2028, 6, 4).unwrap(),
            daily_minutes: 240,
            theme: ThemeChoice::Dark,
            reader_size: 19,
            reader_wide: true,
        }
    }

    #[test]
    fn a_new_store_returns_the_defaults() {
        assert_eq!(Store::in_memory().settings(), Settings::default());
    }

    #[test]
    fn settings_survive_closing_and_reopening_the_file() {
        let dir = std::env::temp_dir().join(format!("civil-mentor-test-{}", std::process::id()));
        let path = dir.join("nested").join("civil-mentor.db");
        let _ = std::fs::remove_dir_all(&dir);

        let store = Store::open(&path).expect("creates the folder and the file");
        assert_eq!(store.path(), Some(path.as_path()));
        store.save_settings(&changed()).unwrap();
        drop(store);

        let store = Store::open(&path).expect("reopens without re-running migrations");
        assert_eq!(store.settings(), changed());
        // Saving twice updates the row instead of failing on the primary key.
        store.save_settings(&Settings::default()).unwrap();
        assert_eq!(store.settings(), Settings::default());
        drop(store);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_unreadable_value_falls_back_to_its_default() {
        let store = Store::in_memory();
        store.save_settings(&changed()).unwrap();
        store.set("exam_date", "not a date").unwrap();
        store.set("theme", "Sepia").unwrap();
        let settings = store.settings();
        assert_eq!(settings.exam_date, Settings::default().exam_date);
        assert_eq!(settings.theme, Settings::default().theme);
        assert_eq!(settings.daily_minutes, 240, "good rows are still read");
    }

    #[test]
    fn readings_are_saved_updated_and_read_back() {
        let store = Store::in_memory();
        assert_eq!(store.readings().unwrap(), HashMap::new());
        let day = |d| NaiveDate::from_ymd_opt(2026, 11, d).unwrap();

        let mut reading = Reading::new(day(14));
        store.save_reading("fr", &reading).unwrap();
        reading.last = day(15);
        reading.done = Some(day(15));
        reading.bookmarked = true;
        store.save_reading("fr", &reading).unwrap();
        store
            .save_reading("monsoon", &Reading::new(day(15)))
            .unwrap();

        let readings = store.readings().unwrap();
        assert_eq!(readings.len(), 2, "saving again updates the row");
        assert_eq!(readings["fr"], reading);
        assert_eq!(readings["monsoon"].done, None);

        // A row with a broken date is skipped; the others still load.
        store
            .conn
            .execute(
                "UPDATE reading SET first = 'soon' WHERE item = 'monsoon'",
                [],
            )
            .unwrap();
        assert_eq!(store.readings().unwrap().len(), 1);
    }

    fn sitting(questions: usize) -> Attempt {
        let at = |h, m| {
            NaiveDate::from_ymd_opt(2026, 11, 15)
                .unwrap()
                .and_hms_opt(h, m, 0)
                .unwrap()
        };
        Attempt {
            id: 0,
            kind: "subject:polity".into(),
            title: "Subject drill: Polity".into(),
            started: at(9, 0),
            finished: None,
            seconds_allowed: questions as u32 * 72,
            seconds_used: 0,
            right_marks: 2.0,
            wrong_marks: 2.0 / 3.0,
            current: 0,
            answers: (0..questions)
                .map(|n| Answer {
                    question: format!("polity-{n:04}"),
                    subject: "polity".into(),
                    topic: "pre.gs.polity.rights".into(),
                    ..Answer::default()
                })
                .collect(),
        }
    }

    #[test]
    fn a_test_is_started_answered_resumed_after_a_crash_and_finished() {
        let dir = std::env::temp_dir().join(format!("civil-mentor-attempt-{}", std::process::id()));
        let path = dir.join("civil-mentor.db");
        let _ = std::fs::remove_dir_all(&dir);

        let store = Store::open(&path).unwrap();
        let mut test = sitting(3);
        test.id = store.start_attempt(&test).unwrap();
        assert!(test.id > 0);

        // The student answers the first question, strikes an option on the
        // second and marks it, and the clock runs.
        test.answers[0].chosen = Some(1);
        test.answers[0].correct = Some(true);
        test.answers[0].confidence = Some(Confidence::FiftyFifty);
        test.answers[0].visited = true;
        test.answers[0].seconds = 40;
        test.seconds_used = 40;
        store.save_answer(&test, 0).unwrap();
        test.current = 1;
        test.answers[1].toggle_strike(2);
        test.answers[1].toggle_strike(3);
        test.answers[1].toggle_strike(3);
        test.answers[1].marked = true;
        test.answers[1].visited = true;
        test.seconds_used = 55;
        store.save_answer(&test, 1).unwrap();
        assert!(test.answers[1].is_struck(2) && !test.answers[1].is_struck(3));
        // The app dies here, before the test is submitted.
        drop(store);

        let store = Store::open(&path).unwrap();
        let unfinished = store.unfinished().unwrap();
        assert_eq!(
            unfinished,
            vec![test.clone()],
            "everything up to the crash is back"
        );
        assert_eq!(unfinished[0].seconds_left(), 3 * 72 - 55);
        assert_eq!(unfinished[0].answered(), 1);
        assert_eq!(store.finished(10).unwrap(), Vec::new());

        test.answers[2].chosen = Some(0);
        test.answers[2].correct = Some(false);
        store.save_answer(&test, 2).unwrap();
        assert!(store.finish_attempt(&test).is_err(), "needs a finish time");
        test.finished = Some(test.started + chrono::Duration::minutes(3));
        store.finish_attempt(&test).unwrap();
        assert_eq!(store.unfinished().unwrap(), Vec::new());
        assert_eq!(store.finished(10).unwrap(), vec![test.clone()]);
        assert_eq!(store.attempt(test.id).unwrap(), Some(test.clone()));
        assert_eq!(store.attempt(test.id + 1).unwrap(), None);

        // What the mentor reads to choose the next questions.
        let past: Vec<(String, bool)> = store
            .answered()
            .unwrap()
            .into_iter()
            .map(|p| (p.question, p.correct))
            .collect();
        assert_eq!(
            past,
            [
                ("polity-0000".to_string(), true),
                ("polity-0002".to_string(), false)
            ]
        );
        drop(store);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn newer_sittings_come_first_and_the_list_is_limited() {
        let store = Store::in_memory();
        for minute in 0..3 {
            let mut test = sitting(1);
            test.title = format!("test {minute}");
            test.id = store.start_attempt(&test).unwrap();
            test.finished = Some(test.started + chrono::Duration::minutes(minute + 1));
            store.finish_attempt(&test).unwrap();
        }
        let titles: Vec<String> = store
            .finished(2)
            .unwrap()
            .into_iter()
            .map(|t| t.title)
            .collect();
        assert_eq!(titles, ["test 2", "test 1"]);
    }

    #[test]
    fn a_path_that_cannot_be_used_is_an_error_not_a_panic() {
        // A regular file where the data folder should be.
        let blocker =
            std::env::temp_dir().join(format!("civil-mentor-blocker-{}", std::process::id()));
        std::fs::write(&blocker, "not a folder").unwrap();
        let err = Store::open(&blocker.join("civil-mentor.db"))
            .err()
            .expect("must not open");
        assert!(err.to_string().contains("could not create"), "{err}");
        std::fs::remove_file(&blocker).unwrap();
    }

    #[test]
    fn a_file_from_a_newer_version_is_refused() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", (MIGRATIONS.len() + 1) as u32)
            .unwrap();
        let err = Store::prepare(conn, None).err().expect("must not open");
        assert!(err.to_string().contains("newer version"), "{err}");
    }
}
