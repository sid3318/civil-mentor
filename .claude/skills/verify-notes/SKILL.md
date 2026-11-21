---
name: verify-notes
description: Verify Civil Mentor's textbook-sized chapters (1 chunk = 1 chapter), written 5 chunks at a time with Gemini 4 Neon XHigh. Checks book-length coverage against the chapter outline & syllabus, factual accuracy, visual hierarchy & rich visual blocks, writes cross-chapter links, and marks chapters verified. Use when the user says "verify the new notes", "verify chunk N" or "/verify-notes".
---

# Verify notes

The argument is a chunk number, a range of chunks (e.g. `1-5`), or a note id. With none, verify every chunk that `python3 tools/notes/notes.py status` shows as written (`•`), oldest first, reporting after each batch. The standard is in `tools/notes/instructions.md`: **1 chunk = 1 full textbook chapter** covering both Prelims and Mains thoroughly, with clear 4-level visual hierarchy (`##`, `###`, `####`, prose/lists) and rich native visual diversity.

For each chapter:

1. Run `python3 tools/notes/notes.py check <chunk>`. Read the chapter (`content/notes/<file>`) in full alongside its `book_ref`, `target_words`, `outline` (Topics & Micro-topics) and `topics` from `tools/notes/manifest.toml`.
2. **Book-Sized Coverage.** Verify every planned `##` Topic and `###` Micro-topic from `outline` and every syllabus leaf topic is covered at genuine textbook depth—never thin summaries. Check both Prelims traps (exact Articles, Amendments, dates, committees, exceptions, majorities) and Mains value-addition (case law, committees, empirical data, constitutional debates, Way Forward).
3. **Facts.** Check every number, date, Article, Amendment, committee, quotation, and recent judgment/law (2024–2026). Settle every `[VERIFY]` against a primary/official source and remove the tag.
4. **Visual Hierarchy & Diversity.** Verify:
   - Clean 4-level hierarchy (`##` numbered Topics, `###` Micro-topics, `####` Sub-points, flowing paragraphs, crisp bullet lists).
   - Rich mix of native callout surfaces (`> [!prelims]`, `> [!mains]`, `> [!news]`, `> [!case]`, `> [!remember]`, `> [!debate]`, `> [!example]`), native visual blocks (````timeline`, ````compare`, ````flow`, ````keyfacts`, ````cases`, ````framework`), editorial quotations, and tables.
   - No images, Mermaid, or raw HTML.
   - Comprehensive distilled quick-revision section after `<!-- distilled -->`.
5. **Connections.** In `tools/notes/links.toml`, ensure 5 to 10 meaningful links from this chapter to other chapters using IDs from `tools/notes/manifest.toml` (`builds-on`, `leads-to`, `same-era`, `compare`, `same-issue`, `example-of`).
6. **Verdict & Cleanup.** Fix any factual or structural gaps, run `python3 tools/notes/notes.py verified <chunk>`, update any matching `note`/`notes` pointers in `content/questions/`, `content/flashcards/`, and `content/current-affairs/`, and remove any superseded old split files replaced by this batch.
7. **Tests.** Run `cargo test --lib embedded_content_has_no_problems` (and `cargo test` whenever UI-tested notes change).
8. Report each chapter's word count, visual elements count, facts verified/corrected, links written, and test status. Commit only when the user asks.
