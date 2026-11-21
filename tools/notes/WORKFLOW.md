# Notes: Textbook Chapter Workflow

151 full textbook chapters in 151 chunks (**1 chunk = 1 complete chapter**), covering all 1,440 leaf topics in the UPSC CSE syllabus. Each chapter replaces fragmented mini-notes with a book-sized chapter comparable to standard UPSC textbooks (Laxmikanth, Spectrum, Ramesh Singh / Vivek Singh, G.C. Leong, Shankar IAS, Nitin Singhania, R.S. Sharma, Satish Chandra, Lexicon), integrating Prelims facts and Mains analysis together.

## What every chapter looks like

- **Book-sized depth:** ~7,000 to 17,000 words per chapter depending on weight (e.g. Fundamental Rights ~15,000 words, Parliament ~16,000 words), following the explicit **Topics (`##`) → Micro-topics (`###` / `####`)** blueprint in `manifest.toml`.
- **Distinct visual hierarchy:** Chapter title (`Inter Display`), `##` Major Topics (`Inter Display` + hairline rule), `###` Micro-topics (`Inter` semibold, indexed in the Inspector sub-outline), `####` Sub-points (accent-tinted `Inter`), flowing `Agrave Pro` textbook prose, accent-marked lists, and italic `Source Serif 4` pull-quotes.
- **Rich visual diversity:**
  - **7 Native Callout Surfaces:** `> [!prelims]`, `> [!mains]`, `> [!news]`, `> [!case]`, `> [!remember]`, `> [!debate]`, `> [!example]`.
  - **6 Native Visual Blocks:** ````timeline`, ````compare`, ````flow`, ````keyfacts`, ````cases`, ````framework` — rendered natively in GPUI while staying searchable plain Markdown.
  - **Structured Tables** (`Inter` tabular layout with horizontal scroll fallback) and **Distilled Quick-Revision** (`<!-- distilled -->`).

## Writing in Batches of 5 Chunks

Chapters are authored 5 chunks at a time using **Gemini 4 Neon XHigh** (either via parallel subagents in Devin or via `python3 tools/notes/notes.py run` with AI Studio), then verified for coverage, factual accuracy, visual diversity, and cross-subject links in `tools/notes/links.toml`.

## Commands

- `python3 tools/notes/notes.py status [all]` — progress across all 151 chapters.
- `python3 tools/notes/notes.py check [1-5 | polity-fundamental-rights | all]` — checks structure, word depth, visual blocks, callouts, `[VERIFY]` tags, links, and 1,440/1,440 syllabus coverage.
- `python3 tools/notes/notes.py verified <chunk-or-range>` — marks verified chapters in `tools/notes/verified.txt`.
- `python3 tools/notes/notes.py run` — interactive clipboard loop for Google AI Studio.
