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

/// One node of the syllabus. `gs2.polity.fundamental-rights` has depth 2.
#[derive(Debug, Clone)]
pub struct Topic {
    pub id: String,
    pub title: String,
    pub depth: usize,
}

#[derive(Debug, Clone)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub topics: Vec<String>,
    pub kind: String,
    pub updated: Option<String>,
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
    /// Index into `options`.
    pub answer: usize,
    pub explanation: String,
    pub trap: Option<String>,
    /// Id of the note that answers it.
    pub note: Option<String>,
    search: String,
}

/// One day of current affairs.
#[derive(Debug, Clone)]
pub struct Brief {
    pub date: NaiveDate,
    pub stories: Vec<Story>,
}

#[derive(Debug, Clone)]
pub struct Story {
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

/// Something search can open. The numbers index into the [`Library`] lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Note(usize),
    Question(usize),
    Story { brief: usize, story: usize },
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
    /// Newest first.
    pub briefs: Vec<Brief>,
    /// Everything wrong with the content files, as "path: message".
    pub problems: Vec<String>,
    topic_index: HashMap<String, usize>,
    note_index: HashMap<String, usize>,
    question_index: HashMap<String, usize>,
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
    answer: String,
    explanation: String,
    trap: Option<String>,
    note: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BriefFile {
    story: Vec<RawStory>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStory {
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
        for (path, _) in &files {
            let known = path == "syllabus.toml"
                || (path.starts_with("notes/") && path.ends_with(".md"))
                || ((path.starts_with("questions/") || path.starts_with("current-affairs/"))
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
            self.topic_index.insert(raw.id.clone(), self.topics.len());
            self.topics.push(Topic {
                depth: raw.id.matches('.').count(),
                id: raw.id,
                title: raw.title,
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
            updated: meta.updated,
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
        let Some(answer) = ["A", "B", "C", "D"]
            .iter()
            .position(|letter| raw.answer.trim().eq_ignore_ascii_case(letter))
        else {
            self.problem(
                path,
                format!(
                    "question `{id}` has answer `{}`, it must be A, B, C or D",
                    raw.answer
                ),
            );
            return None;
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
        for (ix, raw) in file.story.into_iter().enumerate() {
            let question = raw.question.and_then(|mut question| {
                question.id = format!("ca-{date}-{}", ix + 1);
                question.topics = raw.topics.clone();
                question.source.get_or_insert_with(|| {
                    format!("Current affairs, {}", date.format("%-d %b %Y"))
                });
                self.add_question(path, question)
            });
            stories.push(Story {
                search: format!("{}\n{}\n{}", raw.title, raw.context, raw.facts.join("\n"))
                    .to_lowercase(),
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
        self.briefs.push(Brief { date, stories });
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
        for brief in &self.briefs {
            for story in &brief.stories {
                let label = format!("story `{}` ({})", story.title, brief.date);
                for topic in &story.topics {
                    if !self.topic_index.contains_key(topic) {
                        found.push(format!("{label}: unknown topic `{topic}`"));
                    }
                }
                for note in &story.notes {
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

    /// "GS-2 · Polity" for `gs2.polity.fundamental-rights`: the paper and the
    /// subject, which is how the app groups things for the student.
    pub fn subject_label(&self, topic_id: &str) -> String {
        let mut parts = topic_id.split('.');
        let paper = parts.next().unwrap_or_default();
        let subject = parts.next().map(|subject| format!("{paper}.{subject}"));
        let title = |id: &str| {
            self.topic(id)
                .map_or_else(|| id.to_string(), |t| t.title.clone())
        };
        match subject {
            Some(subject) => format!("{} · {}", title(paper), title(&subject)),
            None => title(paper),
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

/// True when any of `topics` is `under` itself or sits below it in the syllabus.
pub fn is_under(topics: &[String], under: &str) -> bool {
    topics.iter().any(|topic| {
        topic
            .strip_prefix(under)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
    })
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
id = "gs2"
title = "GS-2"
[[topic]]
id = "gs2.polity"
title = "Polity"
[[topic]]
id = "gs2.polity.rights"
title = "Fundamental Rights"
"#;

    const NOTE: &str = "---\nid: rights\ntitle: Fundamental Rights\ntopics: [gs2.polity.rights]\n---\n\
        Part III of the Constitution.\n\n<!-- distilled -->\n- Part III.\n";

    const QUESTION: &str = r#"
[[question]]
id = "q1"
topics = ["gs2.polity.rights"]
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
        assert!(library.topics.len() > 50, "syllabus loaded");
        assert!(library.notes.len() >= 4, "notes loaded");
        assert!(library.questions.len() >= 24, "questions loaded");
        assert!(library.briefs.len() >= 2, "briefs loaded");
        assert!(
            library.briefs[0].date > library.briefs[1].date,
            "newest brief first"
        );
        for note in &library.notes {
            assert!(
                !note.distilled.is_empty(),
                "{} has a distilled version",
                note.id
            );
        }
    }

    #[test]
    fn loads_a_small_valid_library() {
        let library = Library::from_files(files(&[
            ("syllabus.toml", SYLLABUS),
            ("notes/gs2/rights.md", NOTE),
            ("questions/polity.toml", QUESTION),
        ]));
        assert_eq!(library.problems, Vec::<String>::new());
        assert_eq!(library.topic("gs2.polity.rights").unwrap().depth, 2);

        let note = library.note("rights").unwrap();
        assert_eq!(note.body, "Part III of the Constitution.");
        assert_eq!(note.distilled, "- Part III.");
        assert_eq!(note.minutes, 1);

        let question = library.question("q1").unwrap();
        assert_eq!(question.answer, 0, "answer letters are case-insensitive");
        assert_eq!(library.subject_label("gs2.polity.rights"), "GS-2 · Polity");
        assert!(is_under(&question.topics, "gs2.polity"));
        assert!(
            !is_under(&question.topics, "gs2.pol"),
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
                "---\nid: b\ntitle: B\ntopic: [gs2]\n---\nText.\n",
            ),
            // No header at all: rejected.
            ("notes/c.md", "Just text.\n"),
            (
                "questions/bad.toml",
                r#"
[[question]]
id = "three-options"
topics = ["gs2.polity"]
stem = "?"
options = ["1", "2", "3"]
answer = "A"
explanation = "."

[[question]]
id = "answer-e"
topics = ["gs2.polity"]
stem = "?"
options = ["1", "2", "3", "4"]
answer = "E"
explanation = "."

[[question]]
id = "missing-note"
topics = ["gs2.polity"]
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
        assert_eq!(library.problems.len(), 8, "{problems}");

        let notes: Vec<&str> = library.notes.iter().map(|note| note.id.as_str()).collect();
        assert_eq!(notes, ["a"]);
        let questions: Vec<&str> = library.questions.iter().map(|q| q.id.as_str()).collect();
        assert_eq!(questions, ["missing-note"]);
    }

    #[test]
    fn a_story_question_joins_the_question_bank() {
        let library = Library::from_files(files(&[
            ("syllabus.toml", SYLLABUS),
            (
                "current-affairs/2026-11-14.toml",
                r#"
[[story]]
title = "Court hears a privacy case"
topics = ["gs2.polity.rights"]
context = "A bench began hearing the case."
why = "Rights."
facts = ["Privacy is part of Article 21."]

[story.question]
stem = "Privacy is part of which Article?"
options = ["14", "19", "21", "32"]
answer = "C"
explanation = "Article 21."
"#,
            ),
        ]));
        assert_eq!(library.problems, Vec::<String>::new());
        let story = &library.briefs[0].stories[0];
        let question = library
            .question(story.question.as_deref().unwrap())
            .unwrap();
        assert_eq!(question.id, "ca-2026-11-14-1");
        assert_eq!(question.topics, ["gs2.polity.rights"]);
        assert_eq!(question.source, "Current affairs, 14 Nov 2026");
        assert_eq!(question.answer, 2);
    }

    #[test]
    fn search_needs_every_word_and_ranks_titles_first() {
        let library = Library::from_files(files(&[
            ("syllabus.toml", SYLLABUS),
            ("notes/gs2/rights.md", NOTE),
            (
                "notes/gs2/writs.md",
                "---\nid: writs\ntitle: Writs\ntopics: [gs2.polity]\n---\n\
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
}
