# Rotation and Recycle Bin

A user replaces a key's secret with `rotate`, which records the secret-change time and keeps the old version in native KDBX history. Removing a key moves it to the Recycle Bin, from where it can be restored or deleted for good.

## Sub-features

- `rotate` replaces the secret, refuses an unchanged or empty value, and records the time.
- `history` lists earlier versions by time and field names, never values.
- `rm-bin` moves a key to the Recycle Bin; `bin` lists it.
- `restore` returns it with the same id, secret and history.
- `purge` deletes for good and writes a deletion record; `purge --older-than-days <n>` is the only retention action.

## How to get to it (user POV)

- CLI: `akm rotate <key> --secret-stdin`, `akm history <key>`, `akm rm <key>`, `akm bin`, `akm restore <key>`, `akm purge <key> --yes`.
- Desktop: a key's detail view (`Rotate secret`, `Move to Recycle Bin`) and the Recycle Bin screen.

## Driving it with akm-verify

Preconditions:

- A session with the key from [vault-and-keys](./vault-and-keys.md).

- **Rotate.** `& $h cli -Session $s -Stdin 'fake-secret-rotated-9876' -- rotate 'Fake OpenAI' --secret-stdin`. Exit 0, and no `fake-` value in the output.
- **Same value refused.** Repeat the same command. Non-zero exit; `get 'Fake OpenAI' --field secret` still prints `fake-secret-rotated-9876`.
- **History.** `& $h cli -Session $s -- history 'Fake OpenAI'`. Exit 0, one earlier version, no `fake-` value.
- **To the bin.** `& $h cli -Session $s -- rm 'Fake OpenAI'`, then `list --json` (no such key) and `bin` (the key, by id and title).
- **Restore.** `& $h cli -Session $s -- restore 'Fake OpenAI'`, then `get 'Fake OpenAI' --field id` prints the same id as before.
- **Purge.** `rm` again, then `& $h cli -Session $s -- purge 'Fake OpenAI' --yes`. `bin` no longer lists it.
- **Interchange.** `tools/verify_keepassxc.py` and `tools/verify_kdbxweb/verify.mjs` repeat these steps against KeePassXC 2.8.0 and kdbxweb.
- **Proof.** The transcript; `audit` shows rotate, remove, restore and purge records with key ids only.

## Gotchas

- History keeps old secrets inside the encrypted vault until a purge. That is by design (unit 2).
- KeePassXC 2.8.0 `merge` ignores deletion records, so merging an old copy brings a purged key back (finding 2-W-01). Do not report a KeePassXC merge as proof that purge holds.
- Times come from the clock with a floor one second after the key's newest known time; two rotations in one second still order correctly.
