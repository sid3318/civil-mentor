"""Checks the notes tool without AI Studio: python3 tools/notes/test_notes.py"""

import datetime
import sys
import tempfile
from pathlib import Path

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).parent))
import notes  # noqa: E402

plan = notes.manifest()
a, b = [n for n in plan if n["chunk"] == 1]


def fresh_folder():
    tmp = Path(tempfile.mkdtemp())
    notes.NOTES, notes.LINKS, notes.VERIFIED = tmp / "notes", tmp / "links.toml", tmp / "verified.txt"


def body(title):
    return (f"## 1. First section\n\nWhy {title} matters, in a paragraph.\n\n| Act | Year |\n|---|---|\n| One | 1773 |\n\n"
            "> **Prelims pointer:** An exact fact.\n\n## 2. Second\n\nMore.\n")


answer = (f"Here are the notes.\n\n=== NOTE: {a['id']} ===\n# {a['title']}\n\n{body(a['title'])}\n=== REVISION ===\n- Fact A.\n\n"
          f"**=== NOTE: {b['id']} ===**\n\n## 1. Cut off in the middle")
rest_of_chunk = f"=== NOTE: {b['id']} ===\n{body(b['title'])}\n<!-- distilled -->\n- Fact B."

# Saving: an answer splits into notes, and a note cut off before its revision list waits.
fresh_folder()
saved, problems = notes.take(answer, plan)
assert saved == [a["id"]] and "stopped before its revision list" in problems[0], (saved, problems)
text = (notes.NOTES / a["file"]).read_text()
assert text.startswith(f"---\nid: {a['id']}\ntitle: ") and '\nsubject: "Polity"\n' in text, text[:200]
assert '\nsection: "Historical background"\norder: 1\n' in text, text[:300]
assert "Here are the notes" not in text and f"# {a['title']}" not in text
assert text.rstrip().endswith("<!-- distilled -->\n\n- Fact A."), text[-80:]
assert notes.lint(a, plan)[0] == [], notes.lint(a, plan)

# A new title, section or place in the manifest reaches the header; the text and the date stay.
(notes.NOTES / a["file"]).write_text(text.replace(f"updated: {datetime.date.today()}", "updated: 2026-01-02"))
notes.refresh([{**a, "title": "A new title", "section": "Another section", "order": 99}])
moved = (notes.NOTES / a["file"]).read_text()
assert 'title: "A new title"\nsubject: "Polity"\nsection: "Another section"\norder: 99\n' in moved, moved[:300]
assert "\nupdated: 2026-01-02\n" in moved and moved.split("---\n", 2)[2] == text.split("---\n", 2)[2]
assert "lost its formatting" in notes.take(f"=== NOTE: {b['id']} ===\nNo headings\n=== REVISION ===\n- x", plan)[1][0]

# Checks: a VERIFY tag or a missing revision list stops a note being marked verified.
(notes.NOTES / a["file"]).write_text(text.replace("1773", "1773 [VERIFY: year]").split("<!-- distilled -->")[0])
errors, _ = notes.lint(a, plan, final=True)
assert any("revision list" in e for e in errors) and any("VERIFY" in e for e in errors), errors
notes.mark_verified([a], plan)
assert a["id"] not in notes.verified()
notes.LINKS.write_text(f'[[link]]\nfrom = "{a["id"]}"\nto = "nowhere"\nkind = "compare"\nwhy = "x"\n')
assert any("bad link" in e for e in notes.lint(a, plan)[0])

# The conveyor, with a pretend clipboard and a pretend user copying things in turn.
fresh_folder()
clip, copies, said = {"value": "something copied earlier"}, [], []
steps = iter([answer, "https://example.com", "## 1. Section\n\nFacts.\n\n=== REVISION ===\n- x", rest_of_chunk])


def user_copies_next(_seconds):
    try:
        clip["value"] = next(steps)
    except StopIteration:
        raise KeyboardInterrupt


notes.paste = lambda: clip["value"]
notes.copy = lambda text: (copies.append(text), clip.update(value=text))
notes.say = said.append
notes.time.sleep = user_copies_next
notes.run(plan)
assert len(copies) == 4, [x[:40] for x in copies]
assert all(f"=== NOTE: {n['id']} ===" in copies[0] for n in (a, b)) and "Neighbouring topics" in copies[0]
assert f"=== NOTE: {b['id']} ===" in copies[1] and a["id"] not in copies[1]
assert copies[2].startswith("Send the same notes again") and b["id"] in copies[2]
assert all(f"=== NOTE: {n['id']} ===" in copies[3] for n in plan if n["chunk"] == 2)
assert said[0] == "Chunk 1 is on your clipboard." and "1 of its 2 notes are left" in said[1]
assert "no marker lines" in said[2] and said[3] == "Chunk 2 is on your clipboard.", said
assert all((notes.NOTES / n["file"]).exists() for n in (a, b))

notes.check([], plan)  # exits 1 if a syllabus topic has no note
print("ok")
