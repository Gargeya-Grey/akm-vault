# AKM design: the key cabinet

The owner asked for a UI designed from nothing, with the same three fonts (2026-11-08). This file is the app's own design system. It wins over the house style on the points below.

## Who it is for

Someone who has API keys in five places and no reason to care about a key manager. The app must be finished in their head after one look: the keys are here, each one is mine, and I can take one out.

## The idea

A key cabinet. A dark spine holds the app together. A shelf holds the projects. Every secret is a real key.

- **Every key has its own cut.** `KeyCut` draws a key whose teeth come from the entry's ID, never from the secret. People find things by shape before they read.
- **The lock is the password rule.** The unlock screen is a pin-tumbler lock with 12 pins, because a new password needs 12 characters. Each character moves the key in and sets a pin. At 12, the shear line turns green.
- **A key wears out.** `LifeRule` shows how much of a key's rotation interval is used, as a row of ticks.

## Tokens

The surface tokens are the house tokens. Three additions, each measured:

| Token | Light | Dark | Use | Contrast |
|---|---|---|---|---|
| `--brass` | `#8A6420` | `#D9B36C` | Key metal only: `KeyCut`, lock pins, illustrations | 4.98 on paper, 4.46 on sage / 9.41, 7.96 |
| `--cabinet` | `#14201C` | `#0A0F0D` | The spine, the lock stage, the secret stage | Text `#F3F1EA` |
| `--cabinet-brass` | `#E3BE78` | `#E3BE78` | Key metal on a cabinet ground | 9.5 / 10.95 |

Muted text on a cabinet ground is `#B9C4BE` (9.34 / 10.77). Focus on a cabinet ground is `--leaf`. A row under the pointer uses `--row-hover`, `#EEF1EC` / `#151D1A`, never the sage that means selected.

## Departures from the house style

- An app shell, not a page: a spine and panes replace the top navigation bar. The theme switch lives in the spine, and keeps its words and its accessible name.
- Glyphs and illustrations are allowed. They carry meaning here, and the owner asked for them.
- The spine and the icons may sit beside words; the words stay.

## Rules that tests hold

- Colour changes are instant. Only `transform` animates. Press scale is `0.97`, and reduced motion removes it.
- A dialog or drawer is fully opaque from its first frame. It slides; it never fades.
- One filled button per view.
- Accessible names are a contract with `tools/verify_windows_app.ps1` and the test suites. Change one only with its test.

## Screens

| Screen | Pane | Notes |
|---|---|---|
| Lock | Stage and form | First run and unlock share it |
| Keys | Shelf and ledger | Grouped by project |
| Key | Drawer | One secret on screen at a time |
| Add or edit | Drawer | Four labelled groups |
| Use keys | Two columns | Take keys out; bring keys in |
| Recycle Bin, Activity, Settings | Ledger | |
