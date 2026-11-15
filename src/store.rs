//! Everything the student does, kept in one SQLite file in the OS app-data folder.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use chrono::NaiveDate;
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
];

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
