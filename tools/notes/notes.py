#!/usr/bin/env python3
"""Civil Mentor's notes: Gemini Neon writes them in Google AI Studio, this tool does the rest.

    python3 tools/notes/notes.py run             moves prompts and answers through the clipboard
    python3 tools/notes/notes.py status [all]    progress
    python3 tools/notes/notes.py check [WHAT]    checks written notes and syllabus coverage
    python3 tools/notes/notes.py verified WHAT   marks notes verified (Claude does this)

WHAT is a note id (polity-emergency), a chunk (7), a range of chunks (7-9) or
"all". See WORKFLOW.md.
"""

import datetime
import json
import os
import re
import subprocess
import sys
import time
import tomllib
from collections import Counter, defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
CONTENT = HERE.parent.parent / "content"
NOTES = CONTENT / "notes"
LINKS = HERE / "links.toml"
VERIFIED = HERE / "verified.txt"

# The lines Neon puts around each note. Bold, backticks or a heading mark around them are fine.
NOTE_LINE = re.compile(r"^[#*`\s]*=+\s*NOTE:\s*([a-z0-9-]+)\s*=+[*`\s]*$", re.M)
REVISION_LINE = re.compile(r"^[#*`\s]*(?:=+\s*REVISION\s*=+|<!--\s*distilled\s*-->)[*`\s]*$", re.M | re.I)
DISTILLED = "<!-- distilled -->"  # where the app's revision mode starts
KINDS = ("builds-on", "leads-to", "same-era", "compare", "same-issue", "example-of")
STOP = {"and", "the", "with", "from", "their", "other", "india", "indian", "issues"}
UTF8 = {**os.environ, "LANG": "en_US.UTF-8"}  # so pbcopy and pbpaste keep › and –

FIX = ("Send the same notes again, unchanged, but put each note's marker line just before it "
       "and the line === REVISION === before each revision list. The marker lines are:\n\n{lines}")


def manifest():
    plan = tomllib.loads((HERE / "manifest.toml").read_text())
    notes = plan["note"]
    for order, n in enumerate(notes, 1):
        n["id"] = n["file"].removesuffix(".md").replace("/", "-")
        n["subject"] = plan["subjects"][n["file"].split("/")[0]]
        n["order"] = order  # where the app lists it
    return notes


def syllabus():
    topics = tomllib.loads((CONTENT / "syllabus.toml").read_text())["topic"]
    kids = defaultdict(list)
    for t in topics:
        if "." in t["id"]:
            kids[t["id"].rsplit(".", 1)[0]].append(t["id"])
    return {t["id"]: t for t in topics}, kids


def leaves(node, kids):
    return [node] if not kids[node] else [x for k in kids[node] for x in leaves(k, kids)]


def verified():
    return set(VERIFIED.read_text().split()) if VERIFIED.exists() else set()


def state(n, done):
    if not (NOTES / n["file"]).exists():
        return "todo"
    return "verified" if n["id"] in done else "written"


def select(what, notes):
    if what == "all":
        return notes
    if re.fullmatch(r"\d+(-\d+)?", what):
        lo, _, hi = what.partition("-")
        picked = [n for n in notes if int(lo) <= n["chunk"] <= int(hi or lo)]
    else:
        picked = [n for n in notes if n["id"] == what.removesuffix(".md").replace("/", "-")]
    if not picked:
        sys.exit(f"Nothing in the manifest matches {what!r}.")
    return picked


# ---- the prompt ---------------------------------------------------------------

def chunk_prompt(targets, notes):
    """The instructions and the notes to write, as one message for AI Studio."""
    by_id, kids = syllabus()
    text = (HERE / "instructions.md").read_text().strip()
    text += (f"\n\n## Write these notes now\n\nToday is {datetime.date.today():%-d %B %Y}. "
             f"Write these {len(targets)} notes in this order, one after another.\n")
    for i, n in enumerate(targets, 1):
        text += f"\n### {i}. {n['title']}\n\nMarker line: === NOTE: {n['id']} ===\n\nSyllabus topics it covers:\n"
        for t in n["topics"]:
            parts = t.split(".")
            text += "- " + " › ".join(by_id[".".join(parts[:k])]["title"] for k in range(2, len(parts) + 1)) + "\n"
            text += "".join(f"  - {by_id[x]['title']}\n" for x in leaves(t, kids) if x != t)
    # Neighbours: the subject's other notes, and any note on the same syllabus chapters.
    mine = {n["id"] for n in targets}
    folders = {n["file"].split("/")[0] for n in targets}
    chapters = {".".join(t.split(".")[:4]) for n in targets for t in n["topics"]}
    near = [m["title"] for m in notes if m["id"] not in mine and (
        m["file"].split("/")[0] in folders or any(".".join(t.split(".")[:4]) in chapters for t in m["topics"]))]
    if near:
        text += ("\n### Neighbouring topics\n\nOther notes in the set cover these, so bring them in "
                 "only where a note above needs them:\n" + "".join(f"- {t}\n" for t in near))
    return text


# ---- the clipboard ------------------------------------------------------------

def paste():
    return subprocess.run(["pbpaste"], capture_output=True, env=UTF8).stdout.decode("utf-8", "replace")


def copy(text):
    subprocess.run(["pbcopy"], input=text.encode("utf-8"), env=UTF8)


def say(message):
    """Prints a message and shows it as a macOS notification, since the terminal is out of sight."""
    print(message, flush=True)
    script = f'display notification "{message.replace(chr(34), chr(39))}" with title "Civil Mentor notes" sound name "Glass"'
    try:
        subprocess.run(["osascript", "-e", script], capture_output=True)
    except OSError:
        pass


def wait(last):
    while (text := paste()) == last:
        time.sleep(1)
    return text


def looks_like_notes(text):
    return ("Marker line:" not in text and re.search(r"^## ", text, re.M)
            and (REVISION_LINE.search(text) or len(text.split()) > 500))


def run(notes):
    print("Leave this running. Paste each prompt into AI Studio, run it, then copy the whole answer "
          "as Markdown: the notes save themselves and the next prompt takes their place on your "
          "clipboard. Ctrl+C stops; running it again carries on.\n", flush=True)
    prompt, last, clip = None, None, paste()  # an answer copied earlier is taken first
    try:
        while True:
            saved, problems = take(clip, notes) if clip != last else ([], [])
            done = verified()
            todo = [n for n in notes if state(n, done) == "todo"]
            if not todo:
                say("Every note is written. Tell Claude to verify the new notes.")
                return
            chunk = min(n["chunk"] for n in todo)
            targets = [n for n in todo if n["chunk"] == chunk]
            fresh = chunk_prompt(targets, notes)
            if saved or fresh != prompt:
                prompt = last = fresh
                copy(prompt)
                total = sum(n["chunk"] == chunk for n in notes)
                rest = "" if len(targets) == total else f": {len(targets)} of its {total} notes are left, so paste it in the same chat"
                say(f"Chunk {chunk} is on your clipboard{rest}.")
            elif problems:
                say(f"Not saved: {problems[0]}.")
                last = clip
            elif clip != last and looks_like_notes(clip):
                last = FIX.format(lines="\n".join(f"=== NOTE: {n['id']} === (for {n['title']})" for n in targets))
                copy(last)
                say("That answer has no marker lines. A request to add them is on your clipboard: paste it in the same chat.")
            else:
                last = clip
            clip = wait(last)
    except KeyboardInterrupt:
        print("\nStopped. Run it again to carry on.")


# ---- saving -------------------------------------------------------------------

def split_answer(text):
    """{note id: its text} for every marker line in a copied answer."""
    marks = list(NOTE_LINE.finditer(text))
    ends = [m.start() for m in marks[1:]] + [len(text)]
    return {m.group(1): text[m.end():end] for m, end in zip(marks, ends)}


def take(text, notes):
    """Saves every note in a copied answer that isn't verified yet. Returns (saved ids, problems)."""
    by_note = {n["id"]: n for n in notes}
    done, saved, problems = verified(), [], []
    for nid, part in split_answer(text).items():
        n = by_note.get(nid)
        if n is None:
            problems.append(f"{nid} is not a note in the manifest")
        elif nid in done:
            problems.append(f"{nid} is verified already, so it was not replaced")
        elif not re.search(r"^## ", part, re.M):
            problems.append(f"{nid} lost its formatting, so copy the answer as Markdown")
        elif not REVISION_LINE.search(part):
            problems.append(f"{nid} stopped before its revision list")
        else:
            save(n, part)
            saved.append(nid)
            print(f"  saved {nid} ({len(body_of(n).split()):,} words)")
            report(*lint(n, notes))
    for p in problems:
        print(f"  not saved: {p}")
    return saved, problems


def save(n, text):
    text = text.strip()
    fenced = re.fullmatch(r"```[a-z]*\n(.*)\n```", text, re.S)
    if fenced:
        text = fenced.group(1).strip()
    first = re.search(r"^## ", text, re.M)
    if first and len(text[:first.start()].split()) <= 60:
        text = text[first.start():]  # a title or "Here are the notes" before the first section
    if text.count("```") % 2 and text.endswith("```"):
        text = text[:-3].rstrip()  # the closing fence of an answer wrapped in one
    body, distilled = (REVISION_LINE.split(text, maxsplit=1) + [""])[:2]
    path = NOTES / n["file"]
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(header(n, datetime.date.today().isoformat()) + "\n" + body.strip() + "\n"
                    + (f"\n{DISTILLED}\n\n{distilled.strip()}\n" if distilled.strip() else ""))


def header(n, updated):
    """A note's front matter, which the app reads. All of it but the date comes from the manifest."""
    lines = [f"id: {n['id']}", f"title: {json.dumps(n['title'], ensure_ascii=False)}",
             f"subject: {json.dumps(n['subject'])}"]
    if "section" in n:
        lines.append(f"section: {json.dumps(n['section'], ensure_ascii=False)}")
    lines += [f"order: {n['order']}", "topics:", *[f"  - {t}" for t in n["topics"]]]
    if n.get("kind", "note") != "note":
        lines.append(f"kind: {n['kind']}")
    return "---\n" + "\n".join(lines) + f"\nupdated: {updated}\n---\n"


def refresh(notes):
    """Rewrites written notes' headers when the manifest changes their title, section, order or topics."""
    for n in notes:
        path = NOTES / n["file"]
        if not path.exists():
            continue
        text = path.read_text()
        _, old, body = text.split("---\n", 2)
        updated = re.search(r"^updated: (\S+)$", old, re.M)
        new = header(n, updated[1] if updated else datetime.date.today().isoformat()) + body
        if new != text:
            path.write_text(new)
            print(f"  updated the header of {n['id']}")


# ---- checks -------------------------------------------------------------------

def body_of(n):
    text = (NOTES / n["file"]).read_text()
    return text.split("---\n", 2)[2] if text.startswith("---\n") else text


def read_links():
    return tomllib.loads(LINKS.read_text()).get("link", []) if LINKS.exists() else []


def lint(n, notes, final=False):
    """Problems with a written note: errors stop it being marked verified."""
    errors, warnings = [], []
    by_id, kids = syllabus()
    text = body_of(n)
    body, _, distilled = text.partition(DISTILLED)
    prose = re.sub(r"```.*?```", "", body, flags=re.S)
    if not re.search(r"^## ", prose, re.M):
        errors.append("no ## sections, so the app has no outline")
    if not distilled.strip():
        errors.append("no revision list")
    tags = len(re.findall(r"\[VERIFY", text))
    if tags:
        (errors if final else warnings).append(f"{tags} [VERIFY] tag(s) to settle")
    if re.search(r"^# ", prose, re.M):
        warnings.append("a '# ' title line; the app shows the title itself")
    if re.search(r"!\[|```mermaid|<(div|img|table)\b", text):  # <br> renders as a line break
        warnings.append("an image, Mermaid or HTML, which the app can't show")
    lower = body.lower()
    for t in n["topics"]:
        for leaf in leaves(t, kids):
            keys = [w for w in re.findall(r"[a-z0-9]+", by_id[leaf]["title"].lower())
                    if len(w) > 3 and w not in STOP]
            if keys and 2 * sum(w in lower for w in keys) < len(keys):
                warnings.append(f"may not cover: {by_id[leaf]['title']}")
    ids = {m["id"] for m in notes}
    for x in read_links():
        if x["from"] == n["id"] and (x["to"] not in ids or x["kind"] not in KINDS):
            errors.append(f"bad link: {x['to']} ({x['kind']})")
    return errors, warnings


def report(errors, warnings):
    for e in errors:
        print(f"    error: {e}")
    for w in warnings:
        print(f"    warn:  {w}")


def check(picked, notes):
    done, bad = verified(), 0
    for n in picked:
        now = state(n, done)
        if now == "todo":
            continue
        errors, warnings = lint(n, notes, final=now == "verified")
        print(f"{n['id']} ({now}, {len(body_of(n).split()):,} words)")
        report(errors, warnings)
        bad += bool(errors)
    by_id, kids = syllabus()
    unknown = sorted({t for n in notes for t in n["topics"] if t not in by_id})
    owned = {x for n in notes for t in n["topics"] if t in by_id for x in leaves(t, kids)}
    every = [t for t in by_id if not kids[t]]
    missing = [t for t in every if t not in owned]
    repeats = [i for i, c in Counter(n["id"] for n in notes).items() if c > 1]
    for t in unknown:
        print(f"manifest: unknown topic {t}")
    for t in missing:
        print(f"manifest: no note covers {t}")
    for i in repeats:
        print(f"manifest: {i} is listed twice")
    print(f"Syllabus: {len(every) - len(missing)} of {len(every)} topics have a note.")
    if bad or unknown or missing or repeats:
        sys.exit(1)


def mark_verified(picked, notes):
    done = verified()
    for n in picked:
        if state(n, done) == "todo":
            print(f"{n['id']}: not written yet")
            continue
        errors, _ = lint(n, notes, final=True)
        if errors:
            print(f"{n['id']}: not marked, still has: {'; '.join(errors)}")
            continue
        done.add(n["id"])
        print(f"{n['id']}: verified")
    VERIFIED.write_text("".join(f"{i}\n" for i in sorted(done)))


def status(notes, everything):
    done = verified()
    chunks = defaultdict(list)
    for n in notes:
        chunks[n["chunk"]].append((n, state(n, done)))
    counts = Counter(s for items in chunks.values() for _, s in items)
    print(f"{len(notes)} notes in {len(chunks)} chunks: {counts['verified']} verified, "
          f"{counts['written']} written and waiting for Claude, {counts['todo']} to write.")
    nxt = min((c for c, items in chunks.items() if any(s == "todo" for _, s in items)), default=None)
    for c, items in sorted(chunks.items()):
        if everything or c == nxt or any(s == "written" for _, s in items):
            print(f"\nchunk {c}" + (" (next)" if c == nxt else ""))
            for n, s in items:
                print(f"  {'✓' if s == 'verified' else '•' if s == 'written' else ' '} {n['id']}")


def main():
    args = sys.argv[1:]
    if not args:
        sys.exit(__doc__)
    command, rest, notes = args[0], args[1:], manifest()
    if command in ("run", "check"):
        refresh(notes)
    if command == "run" and not rest:
        run(notes)
    elif command == "status" and rest in ([], ["all"]):
        status(notes, rest == ["all"])
    elif command == "check" and len(rest) <= 1:
        check(select(rest[0] if rest else "all", notes), notes)
    elif command == "verified" and len(rest) == 1:
        mark_verified(select(rest[0], notes), notes)
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
