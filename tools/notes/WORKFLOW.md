# Notes

304 notes in 148 chunks of two, covering every topic in the UPSC syllabus (all 1,440). Each note serves Prelims and Mains together, and the notes are filed by subject: Polity, Economy, Modern India and so on. Neon writes them in Google AI Studio. A small tool on your Mac carries prompts and answers through the clipboard and saves the notes into the app. Claude checks, fixes and links them.

## What the notes are like

Like the printed notes of a good coaching institute: a teacher explaining the topic in flowing paragraphs, with the facts woven in. Points, tables, quotations and diagrams appear wherever they make things clearer. Boxes pick out what matters:

- **Prelims pointer:** exact facts and traps;
- **Mains angle:** how to use the topic in an answer;
- **Example:** a case, example or key figure;
- **In the news:** a recent development, with its date;
- **Remember:** a mnemonic, exception or common confusion.

Each note ends with a quick-revision list, which the app shows when you click **Distilled**.

## How it works

```text
notes.py run ──► the chunk's prompt is on your clipboard
                          │
                          ▼
           you: paste it in AI Studio and run it
                          │
                          ▼
           you: copy the answer as Markdown
                          │
                          ▼
notes.py run ──► saves the notes into the app and puts
                 the next chunk's prompt on your clipboard
```

## Start, once per sitting

1. In a terminal, run this and leave it running:
   `cd ~/.t3/projects/civil-mentor && python3 tools/notes/notes.py run`
2. In Google AI Studio, open a new chat, choose Gemini 4 Neon and turn on **Grounding with Google Search**.

## Each chunk

1. Paste (⌘V) and run it.
2. When the answer has finished, copy it as Markdown.

A notification says when the notes are saved and the next prompt is ready. Every prompt carries the full instructions, so open a new chat whenever the current one gets long or slow, and switch search grounding on in it. Restart the app to see new notes; they appear in **Notes** under their subject and section.

## Then

Write as many chunks as you like, then tell Claude "verify the new notes". For every note, Claude:

- checks it against its syllabus topics and past Prelims and Mains papers;
- checks the facts and settles every `[VERIFY]`;
- makes sure it reads well and isn't a wall of points;
- adds anything missing;
- writes the connections between notes;
- marks it verified.

Say "commit" when you want the notes committed. If a note needs redoing, Claude gives you a message for the Neon chat. While `run` is going, the new answer saves itself in the same way.

## When a notification says

- **"N of its 2 notes are left":** the answer stopped early. The rest of the chunk's prompt is on your clipboard; paste it in the same chat.
- **"lost its formatting":** you copied plain text. Copy the answer as Markdown.
- **"no marker lines":** a request to add them is on your clipboard; paste it in the same chat.

If AI Studio says you have reached a limit, stop with Ctrl+C and carry on later. `run` starts where you left off.

## Order

Polity, Economy, Modern India, Geography, Environment and disaster management, Ancient India, Medieval India, Art and culture, Science and technology, Governance and social justice, International relations, Internal security, Indian society, World history, Ethics, the current-affairs backbone, the Essay and language papers, then CSAT. The current-affairs backbone dates fastest, so it gets redone before the exam. To change the order, edit the `chunk =` numbers in `manifest.toml`.

## Files

| File | What it is |
|---|---|
| `instructions.md` | What Neon is told in every prompt: the exam, and how the notes should read |
| `manifest.toml` | Every note, with its subject and section, its chunk and its syllabus topics, in the order the app lists them |
| `notes.py` | `run` (the clipboard loop), `status` (progress), `check` and `verified` (used by Claude). `test_notes.py` tests it |
| `links.toml` | The connections between notes, written by Claude |
| `verified.txt` | The notes Claude has verified |

Claude wrote chunks 1 to 5, the first ten Polity notes, as the benchmark for the rest. Neon carries on from chunk 6. The app's old sample notes are gone.

## Later

Visual notes, perhaps as canvases in the app, are a separate phase.
