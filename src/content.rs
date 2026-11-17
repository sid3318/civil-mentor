//! The content pack: syllabus, notes, questions and current-affairs briefs.
//!
//! Content is read-only. It lives as plain files under `content/` and is loaded
//! into memory once at startup. Anything wrong with a file is collected in
//! [`Library::problems`] instead of being dropped silently.

use std::collections::HashMap;
use std::fmt::Display;

use chrono::NaiveDate;
use serde::Deserialize;

/// In debug builds the files are read from disk on every start, so writers see
/// their edits without recompiling. Release builds embed them in the binary.
#[derive(rust_embed::RustEmbed)]
#[folder = "content/"]
struct ContentFiles;

/// One node of the syllabus: a stage, paper, subject, chapter, topic or
/// subtopic. `mains.gs2.polity` (a subject) has depth 2.
#[derive(Debug, Clone)]
pub struct Topic {
    pub id: String,
    pub title: String,
    pub depth: usize,
    /// True when no topic sits below this one. Coverage counts these.
    pub leaf: bool,
    /// UPSC's own wording, where the node matches a line of the official syllabus.
    pub official: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub topics: Vec<String>,
    pub kind: String,
    pub updated: Option<NaiveDate>,
    /// Full text, Markdown.
    pub body: String,
    /// Short version, Markdown. Empty when the note has none.
    pub distilled: String,
    pub minutes: u32,
    search: String,
}

#[derive(Debug, Clone)]
pub struct Question {
    pub id: String,
    pub topics: Vec<String>,
    pub source: String,
    pub stem: String,
    /// Always four.
    pub options: Vec<String>,
    /// Index into `options`. `None` when UPSC dropped the question from
    /// its official answer key: it is shown, but never scored.
    pub answer: Option<usize>,
    pub explanation: String,
    pub trap: Option<String>,
    /// Id of the note that answers it.
    pub note: Option<String>,
    search: String,
}

/// A flashcard: a prompt on the front and the answer on the back.
#[derive(Debug, Clone)]
pub struct Flashcard {
    pub id: String,
    pub topics: Vec<String>,
    pub front: String,
    pub back: String,
    /// Id of the note the card comes from.
    pub note: Option<String>,
    /// Who wrote it, such as "Civil Mentor sample".
    pub source: String,
}

/// A Mains question: the question, its marks and word limit, a model answer
/// and the points a good answer covers.
#[derive(Debug, Clone)]
pub struct MainsQuestion {
    pub id: String,
    /// One of [`MAINS_PAPERS`].
    pub paper: String,
    pub topics: Vec<String>,
    /// Who wrote it, such as "Civil Mentor sample".
    pub source: String,
    /// 10 or 15, with the word limit and clock in [`MAINS_MARKS`].
    pub marks: u32,
    /// The word limit.
    pub words: u32,
    pub question: String,
    /// What a good answer covers. The student ticks the ones they made.
    pub points: Vec<String>,
    /// The model answer, Markdown, within the word limit.
    pub model: String,
}

/// The papers a Mains question can be set in. The Essay paper and the case
/// studies of GS-4 are not in yet.
pub const MAINS_PAPERS: [&str; 4] = ["gs1", "gs2", "gs3", "gs4"];

/// The marks a Mains question can carry, each with its word limit and the
/// minutes on its clock, which keep to the exam's pace of three hours for
/// 250 marks.
pub const MAINS_MARKS: [(u32, u32, u32); 2] = [(10, 150, 7), (15, 250, 11)];

impl MainsQuestion {
    /// The minutes on the clock for its marks.
    pub fn minutes(&self) -> u32 {
        MAINS_MARKS
            .iter()
            .find(|(marks, ..)| *marks == self.marks)
            .map_or(0, |(_, _, minutes)| *minutes)
    }

    /// "GS-2".
    pub fn paper_title(&self) -> String {
        format!("GS-{}", self.paper.trim_start_matches("gs"))
    }
}

/// The words in an answer as an examiner would count them: every run of
/// text with a letter or digit in it, so list bullets and dashes do not count.
pub fn word_count(text: &str) -> usize {
    text.split_whitespace()
        .filter(|word| word.chars().any(char::is_alphanumeric))
        .count()
}

/// The prefix of `source` on a question from a real UPSC paper, followed by its year.
pub const PAPER_SOURCE: &str = "UPSC Prelims ";

impl Question {
    /// The year of the UPSC Prelims paper the question comes from, if it does.
    pub fn year(&self) -> Option<u16> {
        self.source.strip_prefix(PAPER_SOURCE)?.trim().parse().ok()
    }
}

/// One day of current affairs.
#[derive(Debug, Clone)]
pub struct Brief {
    pub date: NaiveDate,
    pub stories: Vec<Story>,
    pub editorials: Vec<Editorial>,
}

#[derive(Debug, Clone)]
pub struct Story {
    /// Stable across edits to the file, so the student's reading of it is kept.
    pub id: String,
    pub title: String,
    pub topics: Vec<String>,
    pub context: String,
    pub why: String,
    pub facts: Vec<String>,
    pub trap: Option<String>,
    pub pyq: Option<String>,
    pub mains: Option<String>,
    pub frame: Vec<String>,
    /// Ids of linked notes.
    pub notes: Vec<String>,
    /// Id of the story's practice question in [`Library::questions`].
    pub question: Option<String>,
    search: String,
}

impl Story {
    /// The key of the story's record in the store's `reading` table, which
    /// it shares with notes.
    pub fn reading_key(&self) -> String {
        format!("story:{}", self.id)
    }
}

/// Our own analysis of an issue in the news. Markdown.
#[derive(Debug, Clone)]
pub struct Editorial {
    pub id: String,
    pub title: String,
    pub topics: Vec<String>,
    pub body: String,
    pub notes: Vec<String>,
    search: String,
}

impl Editorial {
    pub fn reading_key(&self) -> String {
        format!("editorial:{}", self.id)
    }
}

/// Something search can open. The numbers index into the [`Library`] lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Note(usize),
    Question(usize),
    Story { brief: usize, story: usize },
    Editorial { brief: usize, editorial: usize },
}

#[derive(Debug, Clone)]
pub struct Hit {
    pub target: Target,
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Default)]
pub struct Library {
    /// The syllabus, in file order. A parent always comes before its children.
    pub topics: Vec<Topic>,
    pub notes: Vec<Note>,
    pub questions: Vec<Question>,
    /// In file order: files by name, cards as written.
    pub cards: Vec<Flashcard>,
    /// Mains questions in file order: by paper, then as written.
    pub mains: Vec<MainsQuestion>,
    /// Newest first.
    pub briefs: Vec<Brief>,
    /// Everything wrong with the content files, as "path: message".
    pub problems: Vec<String>,
    topic_index: HashMap<String, usize>,
    note_index: HashMap<String, usize>,
    question_index: HashMap<String, usize>,
    card_index: HashMap<String, usize>,
    mains_index: HashMap<String, usize>,
    /// Story and editorial ids, which must be unique across every brief.
    item_ids: std::collections::HashSet<String>,
}

// ---- file formats ---------------------------------------------------------
// `deny_unknown_fields` turns a writer's typo (`topic:` for `topics:`) into a
// reported problem instead of a silently ignored field.

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SyllabusFile {
    topic: Vec<RawTopic>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTopic {
    id: String,
    title: String,
    official: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoteMeta {
    id: String,
    title: String,
    topics: Vec<String>,
    kind: Option<String>,
    updated: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QuestionsFile {
    question: Vec<RawQuestion>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawQuestion {
    // A story's practice question takes its id and topics from the story.
    #[serde(default)]
    id: String,
    #[serde(default)]
    topics: Vec<String>,
    source: Option<String>,
    stem: String,
    options: Vec<String>,
    answer: Option<String>,
    /// A past-paper question UPSC dropped from its key. It has no answer.
    #[serde(default)]
    dropped: bool,
    explanation: String,
    trap: Option<String>,
    note: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CardsFile {
    card: Vec<RawCard>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCard {
    id: String,
    topics: Vec<String>,
    front: String,
    back: String,
    note: Option<String>,
    source: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MainsFile {
    question: Vec<RawMains>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMains {
    id: String,
    paper: String,
    topics: Vec<String>,
    source: Option<String>,
    marks: u32,
    words: u32,
    question: String,
    #[serde(default)]
    points: Vec<String>,
    model: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BriefFile {
    story: Vec<RawStory>,
    #[serde(default)]
    editorial: Vec<RawEditorial>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEditorial {
    id: String,
    title: String,
    topics: Vec<String>,
    body: String,
    #[serde(default)]
    notes: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStory {
    id: String,
    title: String,
    topics: Vec<String>,
    context: String,
    why: String,
    facts: Vec<String>,
    trap: Option<String>,
    pyq: Option<String>,
    mains: Option<String>,
    #[serde(default)]
    frame: Vec<String>,
    #[serde(default)]
    notes: Vec<String>,
    question: Option<RawQuestion>,
}

const DISTILLED_MARKER: &str = "<!-- distilled -->";

impl Library {
    /// The content that ships with the app.
    pub fn embedded() -> Library {
        Library::from_files(ContentFiles::iter().filter_map(|path| {
            let file = ContentFiles::get(&path)?;
            let text = String::from_utf8_lossy(&file.data).into_owned();
            Some((path.into_owned(), text))
        }))
    }

    /// Builds a library from `(path, text)` pairs. Paths are relative to the
    /// content folder and use `/`.
    pub fn from_files(files: impl IntoIterator<Item = (String, String)>) -> Library {
        let mut files: Vec<(String, String)> = files.into_iter().collect();
        files.sort();
        let mut library = Library::default();

        // The syllabus goes first because everything else points into it.
        for (path, text) in &files {
            if path == "syllabus.toml" {
                library.load_syllabus(path, text);
            }
        }
        for (path, text) in &files {
            if path.starts_with("notes/") && path.ends_with(".md") {
                library.load_note(path, text);
            }
        }
        for (path, text) in &files {
            if path.starts_with("questions/") && path.ends_with(".toml") {
                library.load_questions(path, text);
            }
        }
        for (path, text) in &files {
            if path.starts_with("current-affairs/") && path.ends_with(".toml") {
                library.load_brief(path, text);
            }
        }
        for (path, text) in &files {
            if path.starts_with("flashcards/") && path.ends_with(".toml") {
                library.load_cards(path, text);
            }
        }
        for (path, text) in &files {
            if path.starts_with("mains/") && path.ends_with(".toml") {
                library.load_mains(path, text);
            }
        }
        for (path, _) in &files {
            let known = path == "syllabus.toml"
                || (path.starts_with("notes/") && path.ends_with(".md"))
                || ((path.starts_with("questions/")
                    || path.starts_with("current-affairs/")
                    || path.starts_with("flashcards/")
                    || path.starts_with("mains/"))
                    && path.ends_with(".toml"));
            // Editors and Finder leave dotfiles behind; those are not content.
            let hidden = path
                .rsplit('/')
                .next()
                .is_some_and(|name| name.starts_with('.'));
            if !known && !hidden {
                library.problem(path, "not a content file the app knows how to read");
            }
        }

        library
            .briefs
            .sort_by_key(|brief| std::cmp::Reverse(brief.date));
        library.check_links();
        library
    }

    fn problem(&mut self, path: &str, message: impl Display) {
        self.problems.push(format!("{path}: {message}"));
    }

    fn load_syllabus(&mut self, path: &str, text: &str) {
        let file: SyllabusFile = match toml::from_str(text) {
            Ok(file) => file,
            Err(err) => return self.problem(path, toml_error(text, &err)),
        };
        for raw in file.topic {
            if self.topic_index.contains_key(&raw.id) {
                self.problem(path, format!("topic `{}` is listed twice", raw.id));
                continue;
            }
            if let Some((parent, _)) = raw.id.rsplit_once('.')
                && !self.topic_index.contains_key(parent)
            {
                self.problem(
                    path,
                    format!("topic `{}` comes before its parent `{parent}`", raw.id),
                );
                continue;
            }
            if let Some((parent, _)) = raw.id.rsplit_once('.') {
                self.topics[self.topic_index[parent]].leaf = false;
            }
            self.topic_index.insert(raw.id.clone(), self.topics.len());
            self.topics.push(Topic {
                depth: raw.id.matches('.').count(),
                id: raw.id,
                title: raw.title,
                leaf: true,
                official: raw.official,
            });
        }
    }

    fn load_note(&mut self, path: &str, text: &str) {
        let Some((meta, body)) = split_frontmatter(text) else {
            return self.problem(path, "missing the `---` header block at the top");
        };
        let meta: NoteMeta = match serde_saphyr::from_str(meta) {
            Ok(meta) => meta,
            Err(err) => return self.problem(path, format!("header: {err}")),
        };
        if self.note_index.contains_key(&meta.id) {
            return self.problem(path, format!("note id `{}` is already used", meta.id));
        }
        let (body, distilled) = match body.split_once(DISTILLED_MARKER) {
            Some((body, distilled)) => (body.trim(), distilled.trim()),
            None => (body.trim(), ""),
        };
        if body.is_empty() {
            return self.problem(path, "the note has no text");
        }
        let updated = meta.updated.and_then(|text| {
            let date = NaiveDate::parse_from_str(&text, "%Y-%m-%d").ok();
            if date.is_none() {
                self.problem(
                    path,
                    format!("`updated: {text}` is not a date like 2026-11-01"),
                );
            }
            date
        });
        // ponytail: 100 words a minute is a study pace, not a skim. Tune when
        // real reading times are tracked.
        let minutes = (body.split_whitespace().count() as u32)
            .div_ceil(100)
            .max(1);
        self.note_index.insert(meta.id.clone(), self.notes.len());
        self.notes.push(Note {
            search: format!("{}\n{}", meta.title, body).to_lowercase(),
            id: meta.id,
            title: meta.title,
            topics: meta.topics,
            kind: meta.kind.unwrap_or_else(|| "note".into()),
            updated,
            body: body.to_string(),
            distilled: distilled.to_string(),
            minutes,
        });
    }

    fn load_questions(&mut self, path: &str, text: &str) {
        let file: QuestionsFile = match toml::from_str(text) {
            Ok(file) => file,
            Err(err) => return self.problem(path, toml_error(text, &err)),
        };
        for raw in file.question {
            if raw.id.is_empty() {
                self.problem(path, "a question has no id");
                continue;
            }
            self.add_question(path, raw);
        }
    }

    /// Returns the question's id when it was accepted.
    fn add_question(&mut self, path: &str, raw: RawQuestion) -> Option<String> {
        let id = raw.id;
        if self.question_index.contains_key(&id) {
            self.problem(path, format!("question id `{id}` is already used"));
            return None;
        }
        if raw.options.len() != 4 {
            self.problem(
                path,
                format!(
                    "question `{id}` has {} options, it needs 4",
                    raw.options.len()
                ),
            );
            return None;
        }
        let answer = match (raw.dropped, raw.answer) {
            (true, None) => None,
            (true, Some(_)) => {
                self.problem(
                    path,
                    format!("question `{id}` is dropped, so it cannot have an answer"),
                );
                return None;
            }
            (false, None) => {
                self.problem(path, format!("question `{id}` has no answer"));
                return None;
            }
            (false, Some(letter)) => {
                let Some(answer) = ["A", "B", "C", "D"]
                    .iter()
                    .position(|candidate| letter.trim().eq_ignore_ascii_case(candidate))
                else {
                    self.problem(
                        path,
                        format!("question `{id}` has answer `{letter}`, it must be A, B, C or D"),
                    );
                    return None;
                };
                Some(answer)
            }
        };
        if raw.topics.is_empty() {
            self.problem(path, format!("question `{id}` has no topics"));
        }
        self.question_index.insert(id.clone(), self.questions.len());
        self.questions.push(Question {
            search: format!("{}\n{}", raw.stem, raw.explanation).to_lowercase(),
            id: id.clone(),
            topics: raw.topics,
            source: raw.source.unwrap_or_default(),
            stem: raw.stem.trim().to_string(),
            options: raw.options,
            answer,
            explanation: raw.explanation,
            trap: raw.trap,
            note: raw.note,
        });
        Some(id)
    }

    fn load_cards(&mut self, path: &str, text: &str) {
        let file: CardsFile = match toml::from_str(text) {
            Ok(file) => file,
            Err(err) => return self.problem(path, toml_error(text, &err)),
        };
        for raw in file.card {
            let id = raw.id;
            if id.is_empty() || self.card_index.contains_key(&id) {
                self.problem(path, format!("card id `{id}` is empty or already used"));
                continue;
            }
            if raw.front.trim().is_empty() || raw.back.trim().is_empty() {
                self.problem(path, format!("card `{id}` needs a front and a back"));
                continue;
            }
            if raw.topics.is_empty() {
                self.problem(path, format!("card `{id}` has no topics"));
            }
            self.card_index.insert(id.clone(), self.cards.len());
            self.cards.push(Flashcard {
                id,
                topics: raw.topics,
                front: raw.front.trim().to_string(),
                back: raw.back.trim().to_string(),
                note: raw.note,
                source: raw.source.unwrap_or_default(),
            });
        }
    }

    fn load_mains(&mut self, path: &str, text: &str) {
        let file: MainsFile = match toml::from_str(text) {
            Ok(file) => file,
            Err(err) => return self.problem(path, toml_error(text, &err)),
        };
        for raw in file.question {
            let id = raw.id;
            if id.is_empty() || self.mains_index.contains_key(&id) {
                self.problem(
                    path,
                    format!("Mains question id `{id}` is empty or already used"),
                );
                continue;
            }
            if !MAINS_PAPERS.contains(&raw.paper.as_str()) {
                self.problem(
                    path,
                    format!(
                        "Mains question `{id}` has paper `{}`, it must be gs1, gs2, gs3 or gs4",
                        raw.paper
                    ),
                );
                continue;
            }
            // The clock is set by the marks, so other marks have no clock.
            if !MAINS_MARKS
                .iter()
                .any(|&(marks, words, _)| (marks, words) == (raw.marks, raw.words))
            {
                self.problem(
                    path,
                    format!(
                        "Mains question `{id}` has {} marks and {} words, it must be 10 marks \
                         and 150 words or 15 marks and 250 words",
                        raw.marks, raw.words
                    ),
                );
                continue;
            }
            if raw.question.trim().is_empty()
                || raw.model.trim().is_empty()
                || raw.points.is_empty()
            {
                self.problem(
                    path,
                    format!("Mains question `{id}` needs a question, a model answer and points"),
                );
                continue;
            }
            if raw.topics.is_empty() {
                self.problem(path, format!("Mains question `{id}` has no topics"));
            }
            let length = word_count(&raw.model);
            if length > raw.words as usize {
                self.problem(
                    path,
                    format!(
                        "the model answer to `{id}` has {length} words, over its limit of {}",
                        raw.words
                    ),
                );
            }
            self.mains_index.insert(id.clone(), self.mains.len());
            self.mains.push(MainsQuestion {
                id,
                paper: raw.paper,
                topics: raw.topics,
                source: raw.source.unwrap_or_default(),
                marks: raw.marks,
                words: raw.words,
                question: raw.question.trim().to_string(),
                points: raw.points,
                model: raw.model.trim().to_string(),
            });
        }
    }

    fn load_brief(&mut self, path: &str, text: &str) {
        let name = path
            .rsplit('/')
            .next()
            .unwrap_or(path)
            .trim_end_matches(".toml");
        let Ok(date) = NaiveDate::parse_from_str(name, "%Y-%m-%d") else {
            return self.problem(path, "the file name must be a date like 2026-11-14.toml");
        };
        let file: BriefFile = match toml::from_str(text) {
            Ok(file) => file,
            Err(err) => return self.problem(path, toml_error(text, &err)),
        };
        let mut stories = Vec::new();
        for raw in file.story {
            if !self.item_ids.insert(raw.id.clone()) || raw.id.is_empty() {
                self.problem(
                    path,
                    format!("story id `{}` is empty or already used", raw.id),
                );
                continue;
            }
            let question = raw.question.and_then(|mut question| {
                // An explicit id keeps the answers given before stories had ids.
                if question.id.is_empty() {
                    question.id = format!("ca-{}", raw.id);
                }
                question.topics = raw.topics.clone();
                question.source.get_or_insert_with(|| {
                    format!("Current affairs, {}", date.format("%-d %b %Y"))
                });
                self.add_question(path, question)
            });
            stories.push(Story {
                search: format!("{}\n{}\n{}", raw.title, raw.context, raw.facts.join("\n"))
                    .to_lowercase(),
                id: raw.id,
                title: raw.title,
                topics: raw.topics,
                context: raw.context,
                why: raw.why,
                facts: raw.facts,
                trap: raw.trap,
                pyq: raw.pyq,
                mains: raw.mains,
                frame: raw.frame,
                notes: raw.notes,
                question,
            });
        }
        let mut editorials = Vec::new();
        for raw in file.editorial {
            if !self.item_ids.insert(raw.id.clone()) || raw.id.is_empty() {
                self.problem(
                    path,
                    format!("editorial id `{}` is empty or already used", raw.id),
                );
                continue;
            }
            editorials.push(Editorial {
                search: format!("{}\n{}", raw.title, raw.body).to_lowercase(),
                id: raw.id,
                title: raw.title,
                topics: raw.topics,
                body: raw.body.trim().to_string(),
                notes: raw.notes,
            });
        }
        self.briefs.push(Brief {
            date,
            stories,
            editorials,
        });
    }

    /// Reports every reference to a topic or note that does not exist. The
    /// item itself is kept: a broken link is a smaller loss than a lost note.
    fn check_links(&mut self) {
        let mut found = Vec::new();
        for note in &self.notes {
            for topic in &note.topics {
                if !self.topic_index.contains_key(topic) {
                    found.push(format!("note `{}`: unknown topic `{topic}`", note.id));
                }
            }
        }
        for question in &self.questions {
            for topic in &question.topics {
                if !self.topic_index.contains_key(topic) {
                    found.push(format!(
                        "question `{}`: unknown topic `{topic}`",
                        question.id
                    ));
                }
            }
            if let Some(note) = &question.note
                && !self.note_index.contains_key(note)
            {
                found.push(format!("question `{}`: unknown note `{note}`", question.id));
            }
        }
        for card in &self.cards {
            for topic in &card.topics {
                if !self.topic_index.contains_key(topic) {
                    found.push(format!("card `{}`: unknown topic `{topic}`", card.id));
                }
            }
            if let Some(note) = &card.note
                && !self.note_index.contains_key(note)
            {
                found.push(format!("card `{}`: unknown note `{note}`", card.id));
            }
        }
        for question in &self.mains {
            for topic in &question.topics {
                if !self.topic_index.contains_key(topic) {
                    found.push(format!(
                        "Mains question `{}`: unknown topic `{topic}`",
                        question.id
                    ));
                }
            }
        }
        for brief in &self.briefs {
            let items = brief
                .stories
                .iter()
                .map(|s| ("story", &s.id, &s.topics, &s.notes))
                .chain(
                    brief
                        .editorials
                        .iter()
                        .map(|e| ("editorial", &e.id, &e.topics, &e.notes)),
                );
            for (kind, id, topics, notes) in items {
                let label = format!("{kind} `{id}` ({})", brief.date);
                for topic in topics {
                    if !self.topic_index.contains_key(topic) {
                        found.push(format!("{label}: unknown topic `{topic}`"));
                    }
                }
                for note in notes {
                    if !self.note_index.contains_key(note) {
                        found.push(format!("{label}: unknown note `{note}`"));
                    }
                }
            }
        }
        self.problems.extend(found);
    }

    pub fn topic(&self, id: &str) -> Option<&Topic> {
        self.topic_index.get(id).map(|&ix| &self.topics[ix])
    }

    pub fn note(&self, id: &str) -> Option<&Note> {
        self.note_index.get(id).map(|&ix| &self.notes[ix])
    }

    pub fn question(&self, id: &str) -> Option<&Question> {
        self.question_index.get(id).map(|&ix| &self.questions[ix])
    }

    /// A Mains question's place in [`Library::mains`].
    pub fn mains_ix(&self, id: &str) -> Option<usize> {
        self.mains_index.get(id).copied()
    }

    pub fn card(&self, id: &str) -> Option<&Flashcard> {
        self.card_index.get(id).map(|&ix| &self.cards[ix])
    }

    /// Where a question sits in [`Library::questions`].
    pub fn question_ix(&self, id: &str) -> Option<usize> {
        self.question_index.get(id).copied()
    }

    /// The questions that go with a note: those that name it, then those on
    /// a topic the note covers.
    pub fn questions_for(&self, note: &Note) -> Vec<usize> {
        let named = |q: &Question| q.note.as_deref() == Some(note.id.as_str());
        let on_topic = |q: &Question| q.topics.iter().any(|t| covers(&note.topics, t));
        let (mut found, mut rest) = (Vec::new(), Vec::new());
        for (ix, question) in self.questions.iter().enumerate() {
            if named(question) {
                found.push(ix);
            } else if on_topic(question) {
                rest.push(ix);
            }
        }
        found.extend(rest);
        found
    }

    /// Every past paper in the bank, newest first: its year and its
    /// questions in the paper's own order.
    pub fn papers(&self) -> Vec<(u16, Vec<usize>)> {
        let mut papers: Vec<(u16, Vec<usize>)> = Vec::new();
        for (ix, question) in self.questions.iter().enumerate() {
            let Some(year) = question.year() else {
                continue;
            };
            match papers.iter_mut().find(|(y, _)| *y == year) {
                Some((_, questions)) => questions.push(ix),
                None => papers.push((year, vec![ix])),
            }
        }
        papers.sort_by_key(|(year, _)| std::cmp::Reverse(*year));
        papers
    }

    /// The years UPSC asked a question on a topic the note covers, newest first.
    pub fn years_asked(&self, note: &Note) -> Vec<u16> {
        let mut years: Vec<u16> = self
            .questions_for(note)
            .into_iter()
            .filter_map(|ix| self.questions[ix].year())
            .collect();
        years.sort_by_key(|&year| std::cmp::Reverse(year));
        years.dedup();
        years
    }

    /// The brief a student reads on `today`: the newest dated on or before
    /// it, or the oldest when every brief is dated later. By index into
    /// [`Library::briefs`].
    pub fn todays_brief(&self, today: NaiveDate) -> Option<usize> {
        self.briefs
            .iter()
            .position(|brief| brief.date <= today)
            .or_else(|| self.briefs.len().checked_sub(1))
    }

    pub fn brief_on(&self, date: NaiveDate) -> Option<usize> {
        self.briefs.iter().position(|brief| brief.date == date)
    }

    /// The practice questions of a brief, in story order.
    pub fn brief_questions(&self, brief: &Brief) -> Vec<usize> {
        brief
            .stories
            .iter()
            .filter_map(|story| self.question_ix(story.question.as_deref()?))
            .collect()
    }

    /// Every story dated in a month, oldest day first, as `(brief, story)`.
    pub fn month_stories(&self, year: i32, month: u32) -> Vec<(usize, usize)> {
        use chrono::Datelike as _;
        (0..self.briefs.len())
            .rev()
            .filter(|&b| self.briefs[b].date.year() == year && self.briefs[b].date.month() == month)
            .flat_map(|b| (0..self.briefs[b].stories.len()).map(move |s| (b, s)))
            .collect()
    }

    /// "GS-2 · Polity" for `mains.gs2.polity.rights`: the paper and the
    /// subject, the two levels under the stage, which is how the app groups
    /// things for the student.
    pub fn subject_label(&self, topic_id: &str) -> String {
        let parts: Vec<&str> = topic_id.split('.').collect();
        let title = |depth: usize| {
            let id = parts[..=depth].join(".");
            self.topic(&id).map_or(id, |topic| topic.title.clone())
        };
        match parts.len() {
            1 | 2 => title(parts.len() - 1),
            _ => format!("{} · {}", title(1), title(2)),
        }
    }

    /// Case-insensitive search over notes, stories and questions. Every word
    /// of the query must appear; matches in a title rank first.
    ///
    // ponytail: a linear scan over lowercased copies. Fine for tens of
    // thousands of items; move to SQLite FTS5 if the library outgrows that.
    pub fn search(&self, query: &str, limit: usize) -> Vec<Hit> {
        let query = query.to_lowercase();
        let words: Vec<&str> = query.split_whitespace().collect();
        if words.is_empty() {
            return Vec::new();
        }
        // 2 when every word is in the title, 1 when the words are only in the text.
        let score = |title: &str, text: &str| -> u8 {
            if !words.iter().all(|word| text.contains(word)) {
                0
            } else if words.iter().all(|word| title.to_lowercase().contains(word)) {
                2
            } else {
                1
            }
        };
        let first_topic = |topics: &[String]| {
            topics
                .first()
                .map(|topic| self.subject_label(topic))
                .unwrap_or_default()
        };

        let mut hits: Vec<(u8, Hit)> = Vec::new();
        for (ix, note) in self.notes.iter().enumerate() {
            let score = score(&note.title, &note.search);
            if score > 0 {
                let hit = Hit {
                    target: Target::Note(ix),
                    title: note.title.clone(),
                    detail: first_topic(&note.topics),
                };
                hits.push((score, hit));
            }
        }
        for (brief_ix, brief) in self.briefs.iter().enumerate() {
            for (story_ix, story) in brief.stories.iter().enumerate() {
                let score = score(&story.title, &story.search);
                if score > 0 {
                    let hit = Hit {
                        target: Target::Story {
                            brief: brief_ix,
                            story: story_ix,
                        },
                        title: story.title.clone(),
                        detail: brief.date.format("%-d %b %Y").to_string(),
                    };
                    hits.push((score, hit));
                }
            }
        }
        for (brief_ix, brief) in self.briefs.iter().enumerate() {
            for (editorial_ix, editorial) in brief.editorials.iter().enumerate() {
                let score = score(&editorial.title, &editorial.search);
                if score > 0 {
                    let hit = Hit {
                        target: Target::Editorial {
                            brief: brief_ix,
                            editorial: editorial_ix,
                        },
                        title: editorial.title.clone(),
                        detail: format!("Editorial, {}", brief.date.format("%-d %b %Y")),
                    };
                    hits.push((score, hit));
                }
            }
        }
        for (ix, question) in self.questions.iter().enumerate() {
            // A question has no title, so it can only match on its text.
            if score("", &question.search) > 0 {
                let hit = Hit {
                    target: Target::Question(ix),
                    title: one_line(&question.stem, 90),
                    detail: first_topic(&question.topics),
                };
                hits.push((1, hit));
            }
        }
        // Stable sort: within a score, notes stay ahead of stories and questions.
        hits.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
        hits.into_iter().take(limit).map(|(_, hit)| hit).collect()
    }
}

/// True when `id` is `under` itself or sits below it in the syllabus.
pub fn within(id: &str, under: &str) -> bool {
    id.strip_prefix(under)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
}

/// True when any of `topics` is `under` itself or sits below it in the syllabus.
pub fn is_under(topics: &[String], under: &str) -> bool {
    topics.iter().any(|topic| within(topic, under))
}

/// True when content tagged with `topics` covers `topic`: a tag covers its
/// own node and everything below it.
pub fn covers(topics: &[String], topic: &str) -> bool {
    topics.iter().any(|tag| within(topic, tag))
}

/// A part of a note that starts at a `## ` heading. The text before the
/// first heading is a section with no heading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Section<'a> {
    pub heading: Option<&'a str>,
    /// Markdown, starting with the heading line.
    pub text: &'a str,
}

/// Splits Markdown at its `## ` headings, which make the reader's outline.
/// A `## ` line inside a fenced code block is not a heading.
pub fn sections(markdown: &str) -> Vec<Section<'_>> {
    fn push<'a>(sections: &mut Vec<Section<'a>>, heading: Option<&'a str>, text: &'a str) {
        let text = text.trim();
        if !text.is_empty() {
            sections.push(Section { heading, text });
        }
    }
    let (mut sections, mut heading, mut start, mut offset) = (Vec::new(), None, 0, 0);
    let mut fenced = false;
    for line in markdown.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
        } else if !fenced && let Some(title) = line.strip_prefix("## ") {
            push(&mut sections, heading, &markdown[start..offset]);
            heading = Some(title.trim());
            start = offset;
        }
        offset += line.len();
    }
    push(&mut sections, heading, &markdown[start..]);
    sections
}

/// The first line of `text`, cut to about `max` characters.
pub fn one_line(text: &str, max: usize) -> String {
    let line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
        .trim();
    if line.chars().count() <= max {
        line.to_string()
    } else {
        let cut: String = line.chars().take(max).collect();
        format!("{}…", cut.trim_end())
    }
}

/// A TOML parse error with the line it points at, so a writer can find it.
fn toml_error(text: &str, err: &toml::de::Error) -> String {
    match err.span() {
        Some(span) => {
            let line = text[..span.start.min(text.len())].matches('\n').count() + 1;
            format!("line {line}: {}", err.message())
        }
        None => err.message().to_string(),
    }
}

/// Splits a Markdown file into its `---` header block and the text after it.
fn split_frontmatter(text: &str) -> Option<(&str, &str)> {
    let rest = text.trim_start().strip_prefix("---")?;
    let (meta, after) = rest.split_once("\n---")?;
    let body = after.split_once('\n').map_or("", |(_, body)| body);
    Some((meta, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(path, text)| (path.to_string(), text.to_string()))
            .collect()
    }

    const SYLLABUS: &str = r#"
[[topic]]
id = "mains"
title = "Mains"
[[topic]]
id = "mains.gs2"
title = "GS-2"
official = "General Studies-II"
[[topic]]
id = "mains.gs2.polity"
title = "Polity"
[[topic]]
id = "mains.gs2.polity.rights"
title = "Fundamental Rights"
"#;

    const NOTE: &str = "---\nid: rights\ntitle: Fundamental Rights\ntopics: [mains.gs2.polity.rights]\n---\n\
        Part III of the Constitution.\n\n<!-- distilled -->\n- Part III.\n";

    const QUESTION: &str = r#"
[[question]]
id = "q1"
topics = ["mains.gs2.polity.rights"]
stem = "Which writ means 'to have the body'?"
options = ["Habeas corpus", "Mandamus", "Certiorari", "Quo warranto"]
answer = "a"
explanation = "Habeas corpus."
note = "rights"
"#;

    /// The content linter: the files that ship with the app must load cleanly.
    #[test]
    fn embedded_content_has_no_problems() {
        let library = Library::embedded();
        assert_eq!(library.problems, Vec::<String>::new());
        assert!(library.topics.len() > 1500, "syllabus loaded");
        let papers: Vec<&str> = library
            .topics
            .iter()
            .filter(|topic| topic.depth == 1)
            .map(|topic| topic.id.as_str())
            .collect();
        assert_eq!(
            papers,
            [
                "pre.gs",
                "pre.csat",
                "mains.paper-a",
                "mains.paper-b",
                "mains.essay",
                "mains.gs1",
                "mains.gs2",
                "mains.gs3",
                "mains.gs4"
            ],
            "both stages and every compulsory paper"
        );
        assert!(library.notes.len() >= 12, "notes loaded");
        let samples = library
            .questions
            .iter()
            .filter(|q| q.source == "Civil Mentor sample")
            .count();
        assert!(samples >= 96, "about 100 sample questions, found {samples}");
        assert_eq!(library.cards.len(), 30, "every sample flashcard loads");
        for card in &library.cards {
            assert_eq!(card.source, "Civil Mentor sample", "{}", card.id);
            assert!(!card.topics.is_empty(), "{} has a topic", card.id);
        }
        for question in &library.questions {
            assert!(
                question.year().is_some() || !question.source.contains("UPSC"),
                "{} is invented, so it must not carry a UPSC label",
                question.id
            );
        }
        let papers = library.papers();
        assert!(!papers.is_empty(), "at least one real past paper");
        for (year, questions) in &papers {
            assert_eq!(questions.len(), 100, "the {year} paper is complete");
        }
        assert!(library.briefs.len() >= 7, "a week of briefs");
        assert!(
            library.briefs[0].date > library.briefs[1].date,
            "newest brief first"
        );
        for brief in &library.briefs {
            assert_eq!(brief.stories.len(), 5, "{} has five stories", brief.date);
            assert!(
                !brief.editorials.is_empty(),
                "{} has an editorial",
                brief.date
            );
            for story in &brief.stories {
                assert!(story.question.is_some(), "{} has a question", story.id);
                assert!(
                    story
                        .topics
                        .first()
                        .is_some_and(|t| t.starts_with("pre.gs.")),
                    "{} is grouped by a Prelims subject",
                    story.id
                );
            }
        }
        for note in &library.notes {
            assert!(
                !note.distilled.is_empty(),
                "{} has a distilled version",
                note.id
            );
            assert!(
                sections(&note.body).iter().any(|s| s.heading.is_some()),
                "{} has sections for the outline",
                note.id
            );
        }
        for paper in ["mains.gs1", "mains.gs2", "mains.gs3", "mains.gs4"] {
            let notes = library.notes.iter().filter(|n| is_under(&n.topics, paper));
            assert!(notes.count() >= 3, "{paper} has at least three notes");
        }
        assert_eq!(library.mains.len(), 10, "every sample Mains question loads");
        for paper in MAINS_PAPERS {
            assert!(
                library.mains.iter().any(|q| q.paper == paper),
                "{paper} has a Mains question"
            );
        }
        for question in &library.mains {
            assert_eq!(question.source, "Civil Mentor sample", "{}", question.id);
            assert!(
                question.topics.iter().all(|t| t.starts_with("mains.")),
                "{} is tagged with Mains topics",
                question.id
            );
            assert!(
                word_count(&question.model) <= question.words as usize,
                "{} keeps to its limit",
                question.id
            );
            assert!(question.points.len() >= 4, "{} has points", question.id);
        }
    }

    #[test]
    fn words_are_counted_as_an_examiner_counts_them() {
        assert_eq!(word_count(""), 0);
        assert_eq!(word_count("  The Governor  acts\non advice. "), 5);
        assert_eq!(
            word_count("- **Content.** It's 4 per cent — not 6%."),
            7,
            "bullets and dashes are not words"
        );
        assert_eq!(word_count("Article 356\n\n1. Bommai (1994)"), 5);
    }

    #[test]
    fn mains_questions_load_and_bad_ones_are_reported() {
        let question = |id: &str, paper: &str, marks: u32, words: u32, model: &str| {
            format!(
                "[[question]]\nid = \"{id}\"\npaper = \"{paper}\"\n\
                 topics = [\"mains.gs2.polity.rights\"]\nsource = \"Civil Mentor sample\"\n\
                 marks = {marks}\nwords = {words}\nquestion = \"  Discuss Article 21.  \"\n\
                 points = [\"Meaning\", \"Cases\"]\nmodel = \"{model}\"\n\n"
            )
        };
        let long = "word ".repeat(151);
        let mains = [
            question("life", "gs2", 10, 150, "Article 21 protects life."),
            question("life", "gs2", 15, 250, "Again."),
            question("essay", "essay", 15, 250, "An essay."),
            question("case", "gs4", 20, 250, "A case study."),
            question("mismatch", "gs2", 10, 250, "Ten marks, 250 words."),
            question("wordy", "gs2", 10, 150, &long),
            question("bare", "gs2", 15, 250, " "),
        ]
        .concat()
            + "[[question]]\nid = \"lost\"\npaper = \"gs3\"\ntopics = [\"gs9.nowhere\"]\n\
               marks = 15\nwords = 250\nquestion = \"Q\"\npoints = [\"P\"]\nmodel = \"M\"\n";
        let library = Library::from_files(files(&[
            ("syllabus.toml", SYLLABUS),
            ("mains/gs2.toml", &mains),
            (
                "mains/broken.toml",
                "[[question]]\nid = \"x\"\nmarks = \"ten\"\n",
            ),
        ]));

        let problems = library.problems.join("\n");
        for expected in [
            "Mains question id `life` is empty or already used",
            "Mains question `essay` has paper `essay`, it must be gs1, gs2, gs3 or gs4",
            "Mains question `case` has 20 marks and 250 words",
            "Mains question `mismatch` has 10 marks and 250 words",
            "the model answer to `wordy` has 151 words, over its limit of 150",
            "Mains question `bare` needs a question, a model answer and points",
            "Mains question `lost`: unknown topic `gs9.nowhere`",
            "mains/broken.toml:",
        ] {
            assert!(
                problems.contains(expected),
                "missing {expected:?} in:\n{problems}"
            );
        }
        assert_eq!(library.problems.len(), 8, "{problems}");

        let ids: Vec<&str> = library.mains.iter().map(|q| q.id.as_str()).collect();
        assert_eq!(
            ids,
            ["life", "wordy", "lost"],
            "an unknown topic or a long model answer is reported, not dropped"
        );
        let life = &library.mains[library.mains_ix("life").unwrap()];
        assert_eq!(life.question, "Discuss Article 21.");
        assert_eq!(
            (life.minutes(), life.paper_title()),
            (7, "GS-2".to_string())
        );
        assert_eq!(library.mains[2].minutes(), 11, "15 marks get 11 minutes");
        assert_eq!(library.mains[2].source, "");
    }

    #[test]
    fn loads_a_small_valid_library() {
        let library = Library::from_files(files(&[
            ("syllabus.toml", SYLLABUS),
            ("notes/gs2/rights.md", NOTE),
            ("questions/polity.toml", QUESTION),
        ]));
        assert_eq!(library.problems, Vec::<String>::new());
        assert_eq!(library.topic("mains.gs2.polity.rights").unwrap().depth, 3);
        assert!(library.topic("mains.gs2.polity.rights").unwrap().leaf);
        assert!(!library.topic("mains.gs2.polity").unwrap().leaf);

        let note = library.note("rights").unwrap();
        assert_eq!(note.body, "Part III of the Constitution.");
        assert_eq!(note.distilled, "- Part III.");
        assert_eq!(note.minutes, 1);
        assert_eq!(
            library.questions_for(note),
            [0],
            "the question names the note"
        );

        let question = library.question("q1").unwrap();
        assert_eq!(
            question.answer,
            Some(0),
            "answer letters are case-insensitive"
        );
        assert_eq!(
            library.subject_label("mains.gs2.polity.rights"),
            "GS-2 · Polity"
        );
        assert_eq!(library.subject_label("mains.gs2"), "GS-2");
        assert_eq!(
            library.topic("mains.gs2").unwrap().official.as_deref(),
            Some("General Studies-II")
        );
        assert!(covers(&note.topics, "mains.gs2.polity.rights"));
        assert!(
            covers(&["mains.gs2.polity".to_string()], "mains.gs2.polity.rights"),
            "a tag covers everything below it"
        );
        assert!(
            !covers(&note.topics, "mains.gs2.polity"),
            "but not above it"
        );
        assert!(is_under(&question.topics, "mains.gs2.polity"));
        assert!(
            !is_under(&question.topics, "mains.gs2.pol"),
            "only whole path segments match"
        );
    }

    #[test]
    fn reports_what_is_wrong_and_keeps_what_is_usable() {
        let library = Library::from_files(files(&[
            ("syllabus.toml", SYLLABUS),
            // Unknown topic: kept, but reported.
            (
                "notes/a.md",
                "---\nid: a\ntitle: A\ntopics: [gs9.nowhere]\n---\nText.\n",
            ),
            // Misspelt header field: rejected.
            (
                "notes/b.md",
                "---\nid: b\ntitle: B\ntopic: [mains.gs2]\n---\nText.\n",
            ),
            // No header at all: rejected.
            ("notes/c.md", "Just text.\n"),
            // A date that is not a date: kept, but reported.
            (
                "notes/d.md",
                "---\nid: d\ntitle: D\ntopics: [mains.gs2]\nupdated: last week\n---\nText.\n",
            ),
            (
                "questions/bad.toml",
                r#"
[[question]]
id = "three-options"
topics = ["mains.gs2.polity"]
stem = "?"
options = ["1", "2", "3"]
answer = "A"
explanation = "."

[[question]]
id = "answer-e"
topics = ["mains.gs2.polity"]
stem = "?"
options = ["1", "2", "3", "4"]
answer = "E"
explanation = "."

[[question]]
id = "missing-note"
topics = ["mains.gs2.polity"]
stem = "?"
options = ["1", "2", "3", "4"]
answer = "D"
explanation = "."
note = "no-such-note"
"#,
            ),
            ("current-affairs/yesterday.toml", "story = []\n"),
            ("images/map.png", ""),
        ]));

        let problems = library.problems.join("\n");
        for expected in [
            "note `a`: unknown topic `gs9.nowhere`",
            "notes/b.md: header:",
            "notes/c.md: missing the `---` header block",
            "notes/d.md: `updated: last week` is not a date",
            "question `three-options` has 3 options",
            "question `answer-e` has answer `E`",
            "question `missing-note`: unknown note `no-such-note`",
            "current-affairs/yesterday.toml: the file name must be a date",
            "images/map.png: not a content file",
        ] {
            assert!(
                problems.contains(expected),
                "missing {expected:?} in:\n{problems}"
            );
        }
        assert_eq!(library.problems.len(), 9, "{problems}");

        let notes: Vec<&str> = library.notes.iter().map(|note| note.id.as_str()).collect();
        assert_eq!(notes, ["a", "d"]);
        let questions: Vec<&str> = library.questions.iter().map(|q| q.id.as_str()).collect();
        assert_eq!(questions, ["missing-note"]);
    }

    #[test]
    fn flashcards_load_and_bad_ones_are_reported() {
        let library = Library::from_files(files(&[
            ("syllabus.toml", SYLLABUS),
            ("notes/rights.md", NOTE),
            (
                "flashcards/polity.toml",
                r#"
[[card]]
id = "art17"
topics = ["mains.gs2.polity.rights"]
source = "Civil Mentor sample"
front = "  Which Article abolishes untouchability?  "
back = "Article 17."
note = "rights"

[[card]]
id = "art17"
topics = ["mains.gs2.polity"]
front = "Again?"
back = "Again."

[[card]]
id = "no-back"
topics = ["mains.gs2.polity"]
front = "Where is the back?"
back = " "

[[card]]
id = "lost"
topics = ["gs9.nowhere"]
front = "Front"
back = "Back"
note = "no-such-note"
"#,
            ),
            ("flashcards/broken.toml", "[[card]]\nid = 3\n"),
        ]));

        let problems = library.problems.join("\n");
        for expected in [
            "card id `art17` is empty or already used",
            "card `no-back` needs a front and a back",
            "card `lost`: unknown topic `gs9.nowhere`",
            "card `lost`: unknown note `no-such-note`",
            "flashcards/broken.toml:",
        ] {
            assert!(
                problems.contains(expected),
                "missing {expected:?} in:\n{problems}"
            );
        }
        assert_eq!(library.problems.len(), 5, "{problems}");

        let ids: Vec<&str> = library.cards.iter().map(|card| card.id.as_str()).collect();
        assert_eq!(ids, ["art17", "lost"]);
        let card = library.card("art17").unwrap();
        assert_eq!(card.front, "Which Article abolishes untouchability?");
        assert_eq!(card.note.as_deref(), Some("rights"));
        assert_eq!(card.source, "Civil Mentor sample");
        assert_eq!(library.card("lost").unwrap().source, "");
    }

    #[test]
    fn a_dropped_question_has_no_answer_and_papers_group_by_year() {
        let questions = r#"
[[question]]
id = "upsc-2025-001"
topics = ["mains.gs2.polity.rights"]
source = "UPSC Prelims 2025"
stem = "?"
options = ["1", "2", "3", "4"]
dropped = true
explanation = "Dropped."

[[question]]
id = "upsc-2024-001"
topics = ["mains.gs2.polity.rights"]
source = "UPSC Prelims 2024"
stem = "?"
options = ["1", "2", "3", "4"]
answer = "C"
explanation = "."

[[question]]
id = "dropped-with-answer"
topics = ["mains.gs2.polity"]
stem = "?"
options = ["1", "2", "3", "4"]
answer = "A"
dropped = true
explanation = "."

[[question]]
id = "no-answer"
topics = ["mains.gs2.polity"]
stem = "?"
options = ["1", "2", "3", "4"]
explanation = "."
"#;
        let library = Library::from_files(files(&[
            ("syllabus.toml", SYLLABUS),
            ("notes/gs2/rights.md", NOTE),
            ("questions/upsc.toml", questions),
        ]));
        let problems = library.problems.join(
            "
",
        );
        assert!(
            problems.contains("question `dropped-with-answer` is dropped"),
            "{problems}"
        );
        assert!(
            problems.contains("question `no-answer` has no answer"),
            "{problems}"
        );
        assert_eq!(library.problems.len(), 2, "{problems}");
        assert_eq!(library.question("upsc-2025-001").unwrap().answer, None);
        assert_eq!(
            library.question("upsc-2024-001").unwrap().year(),
            Some(2024)
        );
        assert_eq!(
            library.papers(),
            vec![(2025, vec![0]), (2024, vec![1])],
            "newest first"
        );
        let note = library.note("rights").unwrap();
        assert_eq!(
            library.years_asked(note),
            [2025, 2024],
            "the note's topic was asked twice"
        );
    }

    #[test]
    fn a_story_question_joins_the_question_bank() {
        let library = Library::from_files(files(&[
            ("syllabus.toml", SYLLABUS),
            (
                "current-affairs/2026-11-14.toml",
                r#"
[[story]]
id = "2026-11-14-privacy"
title = "Court hears a privacy case"
topics = ["mains.gs2.polity.rights"]
context = "A bench began hearing the case."
why = "Rights."
facts = ["Privacy is part of Article 21."]

[story.question]
stem = "Privacy is part of which Article?"
options = ["14", "19", "21", "32"]
answer = "C"
explanation = "Article 21."

[[story]]
id = "2026-11-14-old"
title = "A story from before stories had ids"
topics = ["mains.gs2.polity.rights"]
context = "."
why = "."
facts = ["."]

[story.question]
id = "ca-2026-11-14-2"
stem = "?"
options = ["1", "2", "3", "4"]
answer = "A"
explanation = "."

[[story]]
id = "2026-11-14-privacy"
title = "The same id again"
topics = ["mains.gs2.polity.rights"]
context = "."
why = "."
facts = ["."]

[[editorial]]
id = "2026-11-14-editorial-privacy"
title = "Privacy needs an oversight body"
topics = ["mains.gs2.polity.rights", "mains.gs9"]
body = """
## The issue
Our own view.
"""
"#,
            ),
        ]));
        assert_eq!(
            library.problems,
            [
                "current-affairs/2026-11-14.toml: story id `2026-11-14-privacy` is empty or already used",
                "editorial `2026-11-14-editorial-privacy` (2026-11-14): unknown topic `mains.gs9`",
            ]
        );
        let brief = &library.briefs[0];
        assert_eq!(brief.stories.len(), 2, "the duplicate is left out");
        assert_eq!(brief.editorials[0].body, "## The issue\nOur own view.");
        assert_eq!(brief.stories[0].reading_key(), "story:2026-11-14-privacy");
        assert_eq!(
            brief.stories[1].question.as_deref(),
            Some("ca-2026-11-14-2"),
            "an explicit question id is kept"
        );
        assert_eq!(library.brief_questions(brief).len(), 2);
        let story = &brief.stories[0];
        let question = library
            .question(story.question.as_deref().unwrap())
            .unwrap();
        assert_eq!(question.id, "ca-2026-11-14-privacy");
        assert_eq!(question.topics, ["mains.gs2.polity.rights"]);
        assert_eq!(question.source, "Current affairs, 14 Nov 2026");
        assert_eq!(question.answer, Some(2));
    }

    #[test]
    fn search_needs_every_word_and_ranks_titles_first() {
        let library = Library::from_files(files(&[
            ("syllabus.toml", SYLLABUS),
            ("notes/gs2/rights.md", NOTE),
            (
                "notes/gs2/writs.md",
                "---\nid: writs\ntitle: Writs\ntopics: [mains.gs2.polity]\n---\n\
                 Writs enforce fundamental rights.\n",
            ),
            ("questions/polity.toml", QUESTION),
        ]));
        let titles = |query: &str| -> Vec<String> {
            library
                .search(query, 10)
                .into_iter()
                .map(|hit| hit.title)
                .collect()
        };
        // Both notes mention the words; the one with them in its title wins.
        assert_eq!(
            titles("RIGHTS fundamental"),
            ["Fundamental Rights", "Writs"]
        );
        assert_eq!(titles("habeas"), ["Which writ means 'to have the body'?"]);
        assert_eq!(titles("rights nothing-matches-this"), Vec::<String>::new());
        assert_eq!(titles("   "), Vec::<String>::new());
        assert_eq!(library.search("rights", 1).len(), 1, "limit is respected");
    }

    #[test]
    fn sections_split_at_level_two_headings_outside_code() {
        let text = "Intro.\n\n## 1. First\nOne.\n\n```\n## not a heading\n```\n## 2. Second\n### Sub\nTwo.\n";
        let found: Vec<(Option<&str>, &str)> =
            sections(text).iter().map(|s| (s.heading, s.text)).collect();
        assert_eq!(
            found,
            [
                (None, "Intro."),
                (
                    Some("1. First"),
                    "## 1. First\nOne.\n\n```\n## not a heading\n```"
                ),
                (Some("2. Second"), "## 2. Second\n### Sub\nTwo."),
            ]
        );
        assert_eq!(
            sections("## Only\nText.").len(),
            1,
            "no empty section before a leading heading"
        );
    }
}
