"""Writes the dotenv fixtures and expected.json.

The `akm` values are written by hand from docs/design/unit3.md section 2. The `reader` values
come from python-dotenv, run without interpolation, over the exact bytes of each file. A case
where they differ must carry a `note` that says why AKM differs on purpose.

    python generate.py            rewrite the files and expected.json
    python generate.py --check    fail when a file or expected.json differs from the table

Only fake values appear here. Run from any folder; paths follow this file.
"""
import importlib.metadata
import json
import pathlib
import sys

from dotenv.parser import parse_stream

HERE = pathlib.Path(__file__).resolve().parent
BOM = "\ufeff"


def ok(*pairs):
    return {"entries": [list(pair) for pair in pairs]}


def err(fragment):
    return {"error": fragment}


# (file name, bytes, akm expectation, note when python-dotenv differs)
CASES = [
    ("plain", "A=1\nB=two words\nC=x/y:z@w+v.u-t_s\n".encode(),
     ok(("A", "1"), ("B", "two words"), ("C", "x/y:z@w+v.u-t_s")), None),

    ("comments",
     (
         "# full line comment\n"
         "   # indented comment\n"
         "TOKEN=abc # note\n"
         "HASH=abc#nocomment\n"
         'QUOTED="a # b" # trailing\n'
         "SQUOTED='a # b'  # trailing\n"
         "TAB=abc\t# note\n"
         "HASHFIRST=#value\n"
         "\n"
     ).encode(),
     ok(("TOKEN", "abc"), ("HASH", "abc#nocomment"), ("QUOTED", "a # b"), ("SQUOTED", "a # b"),
        ("TAB", "abc"), ("HASHFIRST", "#value")), None),

    ("comment_only_value", b"EMPTYC= # only a comment\nNEXT=1\n",
     ok(("EMPTYC", ""), ("NEXT", "1")),
     "A # that follows the equals sign and spaces starts a comment, as in a shell. "
     "python-dotenv keeps the comment text as the value."),

    ("quotes",
     (
         "NONE=plain value\n"
         "SINGLE='single value'\n"
         'DOUBLE="double value"\n'
         "EMPTY_S=''\n"
         'EMPTY_D=""\n'
         'DQ_WITH_SQ="it\'s"\n'
         "SQ_WITH_DQ='say \"hi\"'\n"
         'MID=ab"cd\n'
         "APOS=it's\n"
         'PADDED="  padded  "\n'
     ).encode(),
     ok(("NONE", "plain value"), ("SINGLE", "single value"), ("DOUBLE", "double value"),
        ("EMPTY_S", ""), ("EMPTY_D", ""), ("DQ_WITH_SQ", "it's"), ("SQ_WITH_DQ", 'say "hi"'),
        ("MID", 'ab"cd'), ("APOS", "it's"), ("PADDED", "  padded  ")), None),

    ("escapes_double",
     (
         r'A="a\nb\tc\rd\"e\\f"' "\n"
         r'B="\$HOME and \x41 and \u00e9 and \q"' "\n"
         r'C="C:\\tmp\\new"' "\n"
     ).encode(),
     ok(("A", 'a\nb\tc\rd"e\\f'), ("B", r"\$HOME and \x41 and \u00e9 and \q"),
        ("C", "C:\\tmp\\new")), None),

    ("escapes_extra",
     (r'A="bell\a back\b form\f vert\v apos\'"' "\n").encode(),
     ok(("A", r"bell\a back\b form\f vert\v apos\'")),
     "AKM decodes only \\n, \\r, \\t, \\\" and \\\\ in double quotes. python-dotenv also decodes "
     "\\a, \\b, \\f, \\v and \\'. A secret that holds a backslash and one of those letters "
     "would change silently."),

    ("single_quote_literal", (r"A='a\nb'" "\n" r"B='C:\tmp'" "\n").encode(),
     ok(("A", r"a\nb"), ("B", r"C:\tmp")), None),

    ("single_quote_double_backslash", (r"A='back\\slash'" "\n").encode(),
     ok(("A", r"back\\slash")),
     "A single-quoted value is literal, as in a shell and in the Node dotenv package, so two "
     "backslashes stay two. python-dotenv turns them into one."),

    ("single_quote_backslash_quote", (r"A='a\'b'" "\n").encode(),
     err("line 1"),
     "A single quote ends a single-quoted value; there is no \\' escape. The text after the "
     "closing quote is then an error in AKM. python-dotenv reads \\' as an escaped quote."),

    ("export",
     b"export A=1\nexport   B=2\nexport\tC=3\n",
     ok(("A", "1"), ("B", "2"), ("C", "3")), None),

    ("export_without_value", b"export A\n", err("line 1"),
     "A name with no equals sign is an error in AKM. python-dotenv gives it the value None."),

    ("spacing", b"A = 1\nB= 2\nC =3\n  D=4\n\tE=5\n",
     ok(("A", "1"), ("B", "2"), ("C", "3"), ("D", "4"), ("E", "5")), None),

    ("trailing_space", b'A=abc   \nB="abc "   \n',
     ok(("A", "abc"), ("B", "abc ")), None),

    ("equals_in_value", b"A=b=c=d\nURL=https://x.example/p?a=1&b=2\n",
     ok(("A", "b=c=d"), ("URL", "https://x.example/p?a=1&b=2")), None),

    ("empty", b'A=\nB=""\nC=\'\'\nD=   \n',
     ok(("A", ""), ("B", ""), ("C", ""), ("D", "")), None),

    ("unicode", "A=h\u00e9llo w\u00f6rld\nB=\"\u65e5\u672c\u8a9e\"\n".encode(),
     ok(("A", "h\u00e9llo w\u00f6rld"), ("B", "\u65e5\u672c\u8a9e")), None),

    ("no_final_newline", b"A=1\nB=2", ok(("A", "1"), ("B", "2")), None),

    ("crlf", b'A=1\r\nB=2\r\nC="x\r\ny"\r\nD=last\r\n',
     ok(("A", "1"), ("B", "2"), ("C", "x\r\ny"), ("D", "last")),
     "A CRLF inside a quoted value stays as written. python-dotenv reads the file in text mode "
     "and turns it into a line feed."),

    ("bom", (BOM + "A=1\nB=2\n").encode(),
     ok(("A", "1"), ("B", "2")),
     "AKM drops one leading byte order mark. python-dotenv, opened as plain UTF-8, keeps it in "
     "the first name."),

    ("multiline_double",
     b'PEM="-----BEGIN FAKE KEY-----\nline-one\nline-two\n-----END FAKE KEY-----"\nAFTER=1\n',
     ok(("PEM", "-----BEGIN FAKE KEY-----\nline-one\nline-two\n-----END FAKE KEY-----"),
        ("AFTER", "1")), None),

    ("multiline_single", b"A='m1\nm2'\nB=2\n", ok(("A", "m1\nm2"), ("B", "2")), None),

    ("multiline_comment_marks_kept", b'A="x\n# not a comment\ny" # real comment\nB=2\n',
     ok(("A", "x\n# not a comment\ny"), ("B", "2")), None),

    ("interpolation_is_literal",
     b"A=${HOME}/x\nB=\"$A\"\nC='${A}'\nD=$(echo hi)\nE=`echo hi`\nF=%USERPROFILE%\n",
     ok(("A", "${HOME}/x"), ("B", "$A"), ("C", "${A}"), ("D", "$(echo hi)"),
        ("E", "`echo hi`"), ("F", "%USERPROFILE%")),
     None),

    ("duplicates", b"A=1\nA=2\n", err("more than once"),
     "A repeated name is an error in AKM. python-dotenv keeps the last value."),

    ("lowercase_name", b"lower=1\n", err("line 1"),
     "Names are A-Z, 0-9 and _ only, and may not start with a digit. python-dotenv accepts "
     "any name."),

    ("digit_first_name", b"1A=1\n", err("line 1"),
     "Same rule as lowercase_name."),

    ("hyphen_name", b"A-B=1\n", err("line 1"),
     "Same rule as lowercase_name."),

    ("malformed_no_equals", b"A=1\nnot a line\nB=2\n", err("line 2"),
     "A line that is not NAME=value is an error in AKM. python-dotenv skips it."),

    ("malformed_unterminated_double", b'A=1\nB="never closed\nC=3\n', err("line 2"),
     "AKM refuses an unterminated quote. python-dotenv skips a malformed line and goes on."),

    ("malformed_unterminated_single", b"A='never closed\nB=2\n", err("line 1"),
     "Same rule as malformed_unterminated_double."),

    ("malformed_after_closing_quote", b'A="x" y\nB=2\n', err("line 1"),
     "Text after a closing quote that is not a comment is an error in AKM. python-dotenv "
     "skips the line."),

    ("malformed_single_after_closing_quote", b"A='x'y\nB=2\n", err("line 1"),
     "Same rule as malformed_after_closing_quote."),

    ("malformed_nul", b"A=a\x00b\n", err("line 1"),
     "A NUL character is an error in AKM."),

    ("malformed_lone_cr", b"A=1\rB=2\n", err("line 1"),
     "AKM ends a line only at a line feed or a CR LF pair. A lone CR is an error. "
     "python-dotenv reads a lone CR as a line end."),
]


# (value, the line that dotenv::line writes for NAME "A"). python-dotenv must read each line
# back to the same value.
WRITER = [
    ("abc-123", "A=abc-123"),
    ("", "A=''"),
    ("two words", "A='two words'"),
    ("a # b", "A='a # b'"),
    ("#start", "A='#start'"),
    ("${HOME}/x", "A='${HOME}/x'"),
    ("it's", 'A="it\'s"'),
    ('say "hi"', 'A="say \\"hi\\""'),
    ("back\\slash", 'A="back\\\\slash"'),
    ("two\nlines", 'A="two\\nlines"'),
    ("cr\r\nlf", 'A="cr\\r\\nlf"'),
    ("tab\there", "A='tab\there'"),
    ("  padded  ", "A='  padded  '"),
    ("h\u00e9llo \u65e5\u672c", "A='h\u00e9llo \u65e5\u672c'"),
]


def read_with_reference(path):
    with open(path, encoding="utf-8") as stream:
        bindings = list(parse_stream(stream))
    entries = [[b.key, b.value] for b in bindings if b.key is not None]
    return {"entries": entries, "skipped": sum(1 for b in bindings if b.error)}


def main():
    check = "--check" in sys.argv
    for name, data, _, _ in CASES:
        target = HERE / f"{name}.dotenv"
        if check:
            if not target.exists() or target.read_bytes() != data:
                sys.exit(f"{name} differs from the table")
        else:
            target.write_bytes(data)
    cases = []
    for name, _, akm, note in CASES:
        cases.append({
            "file": f"{name}.dotenv",
            "akm": akm,
            "reader": read_with_reference(HERE / f"{name}.dotenv"),
            "note": note,
        })
    document = {
        "reader": {
            "name": "python-dotenv",
            "version": importlib.metadata.version("python-dotenv"),
            "call": "dotenv.parser.parse_stream(open(path, encoding='utf-8')); no interpolation",
        },
        "cases": cases,
        "writer": [{"value": value, "line": line} for value, line in WRITER],
    }
    for value, line in WRITER:
        from io import StringIO
        back = [b.value for b in parse_stream(StringIO(line + "\n"))]
        if back != [value]:
            sys.exit(f"the reference reader does not read {line!r} back to {value!r}: {back!r}")
    text = json.dumps(document, indent=2, ensure_ascii=False) + "\n"
    target = HERE / "expected.json"
    if check:
        if not target.exists() or target.read_text(encoding="utf-8") != text:
            sys.exit("expected.json differs from the table")
    else:
        target.write_text(text, encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main()
