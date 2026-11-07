These encrypted fixtures contain fake test credentials only.

`kdbx3.kdbx` and `kdbx40.kdbx` were generated with the independent `kdbxweb`
implementation. They contain protected custom fields, history, an attachment,
custom metadata, and an unrelated group. A binary is shared with another entry
and a history-only attachment reference. Their AES KDF uses only ten rounds
for fast tests. This is deliberately unsuitable for real secrets.

Password: `correct horse battery staple`.

To regenerate with an already installed `kdbxweb`, run
`node generate.cjs <absolute-path-to-kdbxweb>`. Cargo tests need only the
checked-in binary fixtures and do not invoke Node or install dependencies.

## dotenv/

`dotenv/*.dotenv` are fake dotenv files that pin the AKM dotenv profile
(`docs/design/unit3.md`, section 2). `dotenv/expected.json` holds, for each file, the result
that AKM must give, the result of the reference reader `python-dotenv` (version recorded in
the file), and a note wherever the two differ on purpose. `dotenv/generate.py` writes the
files and the JSON; `python generate.py --check` fails when they drift. Cargo tests need
only the checked-in files and do not run Python. The files use the extension `.dotenv`
because `.gitignore` blocks every `*.env` file.
