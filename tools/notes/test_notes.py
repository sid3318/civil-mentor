"""Checks the notes tool without AI Studio: python3 tools/notes/test_notes.py"""

import datetime
import sys
import tempfile
from pathlib import Path

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).parent))
import notes  # noqa: E402

plan = notes.manifest()
a = [n for n in plan if n["chunk"] == 1][0]
b = [n for n in plan if n["chunk"] == 2][0]


def fresh_folder():
    tmp = Path(tempfile.mkdtemp())
    notes.NOTES, notes.LINKS, notes.VERIFIED = tmp / "notes", tmp / "links.toml", tmp / "verified.txt"


def body(title):
    return (
        f"## 1. First topic\n\n### Micro-topic A\n\nWhy {title} matters, in a paragraph.\n\n"
        "```timeline\n1773 | Regulating Act | First control\n1833 | Charter Act | Centralisation\n```\n\n"
        "```compare\nCompany | Crown\nRule :: 1773 | 1858\n```\n\n"
        "```keyfacts\n1773 | First Act | Regulating Act\n```\n\n"
        "> [!prelims]\n> An exact fact.\n\n"
        "> [!mains]\n> Analytical angle.\n\n"
        "| Act | Year |\n|---|---|\n| One | 1773 |\n\n"
        "## 2. Second topic\n\n### Micro-topic B\n\nMore.\n"
    )


answer = (
    f"Here is the chapter.\n\n=== NOTE: {a['id']} ===\n# {a['title']}\n\n{body(a['title'])}\n"
    "=== REVISION ===\n- Fact A.\n"
)
cut_answer = f"=== NOTE: {b['id']} ===\n\n## 1. Cut off in the middle"
rest_of_chunk = f"=== NOTE: {b['id']} ===\n{body(b['title'])}\n<!-- distilled -->\n- Fact B."

# Verify 1 chunk = 1 chapter across the entire manifest.
chunks = [n["chunk"] for n in plan]
assert len(chunks) == len(set(chunks)), "each chunk must map to exactly one textbook chapter"
assert all(n.get("outline") for n in plan), "every chapter must define its topics & microtopics outline"

# Saving: an answer saves cleanly, and a cut-off chapter waits.
fresh_folder()
saved, problems = notes.take(answer, plan)
assert saved == [a["id"]] and not problems, (saved, problems)
saved2, problems2 = notes.take(cut_answer, plan)
assert not saved2 and "stopped before its revision list" in problems2[0], (saved2, problems2)
text = (notes.NOTES / a["file"]).read_text()
assert text.startswith(f"---\nid: {a['id']}\ntitle: ") and '\nsubject: "Polity"\n' in text, text[:200]
assert '\nsection: "Part I: Constitutional Framework"\norder: 1\n' in text, text[:300]
assert "Here is the chapter" not in text and f"# {a['title']}" not in text
assert text.rstrip().endswith("<!-- distilled -->\n\n- Fact A."), text[-80:]
assert notes.lint(a, plan)[0] == [], notes.lint(a, plan)

# Prompt includes Book Reference, Target Length, and Topics/Micro-topics outline.
prompt_text = notes.chunk_prompt([a], plan)
assert "Book Reference:" in prompt_text and "Target Length:" in prompt_text
assert "Required Chapter Blueprint" in prompt_text and f"=== NOTE: {a['id']} ===" in prompt_text

# A new title, section or place in the manifest reaches the header; the text and the date stay.
(notes.NOTES / a["file"]).write_text(text.replace(f"updated: {datetime.date.today()}", "updated: 2026-01-02"))
notes.refresh([{**a, "title": "A new title", "section": "Another section", "order": 99}])
moved = (notes.NOTES / a["file"]).read_text()
assert 'title: "A new title"\nsubject: "Polity"\nsection: "Another section"\norder: 99\n' in moved, moved[:300]
assert "\nupdated: 2026-01-02\n" in moved and moved.split("---\n", 2)[2] == text.split("---\n", 2)[2]
assert "lost its formatting" in notes.take(f"=== NOTE: {b['id']} ===\nNo headings\n=== REVISION ===\n- x", plan)[1][0]

# Checks: a VERIFY tag or a missing revision list stops a chapter being marked verified.
(notes.NOTES / a["file"]).write_text(text.replace("1773", "1773 [VERIFY: year]").split("<!-- distilled -->")[0])
errors, _ = notes.lint(a, plan, final=True)
assert any("revision list" in e for e in errors) and any("VERIFY" in e for e in errors), errors
notes.mark_verified([a], plan)
assert a["id"] not in notes.verified()
notes.LINKS.write_text(f'[[link]]\nfrom = "{a["id"]}"\nto = "nowhere"\nkind = "compare"\nwhy = "x"\n')
assert any("bad link" in e for e in notes.lint(a, plan)[0])

# The conveyor with a pretend clipboard.
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
assert f"=== NOTE: {a['id']} ===" in copies[0] and "Neighbouring chapters" in copies[0]
assert f"=== NOTE: {b['id']} ===" in copies[1] and a["id"] not in copies[1]
assert copies[2].startswith("Send the same chapter again") and b["id"] in copies[2]
assert all((notes.NOTES / n["file"]).exists() for n in (a, b))

notes.check([], plan)  # exits 1 if a syllabus topic has no chapter
print("ok")
