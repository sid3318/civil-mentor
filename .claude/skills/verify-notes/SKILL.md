---
name: verify-notes
description: Verify Civil Mentor's new notes, which the user writes with Gemini Neon in AI Studio and `notes.py run` saves. Each note covers a topic for Prelims and Mains together. Checks coverage, facts and readability, writes the connections between notes and marks them verified. Use when the user says "verify the new notes", "verify chunk N" or "/verify-notes".
---

# Verify notes

The argument is a chunk number, a range of chunks or a note id. With none, verify every chunk that `python3 tools/notes/notes.py status` shows with written (•) notes, oldest first, reporting after each chunk. The standard is in `tools/notes/instructions.md`: one note per topic for Prelims and Mains together, reading like good coaching material, with flowing explanation as the backbone.

For each note:

1. Run `python3 tools/notes/notes.py check <chunk>`. Read the note (`content/notes/<file>`) in full, with its syllabus topics from `tools/notes/manifest.toml`.
2. **Coverage.** Every syllabus topic is covered for both stages: the facts Prelims can demand, and the understanding, dimensions, judgments, committees, data and way forward Mains rewards. Recall what UPSC has asked on these topics (Prelims about 2011 onwards, Mains about 2013 onwards; search the web for recent papers when unsure), and make sure the note equips the student for each such question. Add what is missing.
3. **Facts.** Check every number, date, Article, name, place, quotation and attribution. Settle each `[VERIFY]` with a web search against an official or primary source, then remove the tag. Confirm recent items and their dates. Drop a quotation you cannot confirm word for word, or put it in reported speech.
4. **Readability.** The note reads like a teacher explaining: paragraphs that flow, with points, tables, boxes and quotations where they help. Turn runs of bare bullets into prose where explanation is needed. Cut padding, repetition, and what belongs to a neighbouring note.
5. **The app.** The note has `##` sections. Its tables and text diagrams render, the diagrams are no wider than about 60 characters, and there are no images, Mermaid or HTML. The revision list after `<!-- distilled -->` holds the points most worth remembering.
6. **Connections.** In `tools/notes/links.toml`, add four to ten links from this note to other notes, written or not, using ids from the manifest:

   ```toml
   [[link]]
   from = "modern-india-british-conquest"
   to = "modern-india-revolt-1857"
   kind = "leads-to"
   why = "The annexations of Jhansi (1853, doctrine of lapse) and Awadh (1856, misgovernance) gave the revolt two of its leaders, Rani Lakshmibai and Begum Hazrat Mahal."
   ```

   The kinds are:
   - `builds-on`: read the other note first.
   - `leads-to`: a cause, or what followed.
   - `same-era`: contemporaries.
   - `compare`: asked together, or confused.
   - `same-issue`: the same issue in another subject or paper.
   - `example-of`: a case of the other.

   The reason names the specific link, as a mentor would. Link across subjects and papers, and to contemporaries in history and culture.
7. **Verdict.** If the note is sound, fix what is left by hand and run `python3 tools/notes/notes.py verified <note-id>`. If it is too weak to fix by hand, write the user a message to paste into the Neon chat. Say what to redo, and ask for the whole note again, starting with the line `=== NOTE: <note-id> ===` and ending with `=== REVISION ===` and its list. With `notes.py run` going, the answer saves itself. Verify it once it is saved.

Then:

- A note's title, section and place in the list come from `tools/notes/manifest.toml`. To change them, edit the manifest: `notes.py check` rewrites the headers of written notes to match.
- Where a question, flashcard or story in `content/` is plainly about a newly verified note, point its `note` or `notes` field at that note's id. Questions also reach notes through their topics without it.
- Run `cargo test --lib embedded_content_has_no_problems`. The UI test in `tests/screens.rs` opens four of Claude's benchmark notes (`polity-fundamental-rights`, `polity-rights-freedom`, `polity-salient-features-preamble` and `polity-making-of-the-constitution`). If any of them is edited, run the full `cargo test` as well.
- Report each note's verdict, the facts you corrected (with sources), what you added, cut or rewrote, and the links you made.
- Commit only when the user asks, and commit straight to `main`.
