# AKM design: a quiet instrument

The owner chose this direction on 2026-11-08 and gave the design of their product Odicto as the model. The three fonts stay: Instrument Sans for the interface, IBM Plex Mono for data, Literata for notes. This file is the app's own design system. It wins over the house style on the points below.

## Who it is for

Someone who has API keys in five places and no reason to care about a key manager. The app must be calm enough to leave open all day, and honest about the state of each key.

## The idea

A quiet instrument on a dark desk. Soft light and glass for the surfaces. Hairlines and exact marks for the data.

- **Depth comes from light.** A panel is glass with a light top rim and a deep soft shadow, not a border. A field is a sunken well.
- **One accent means "this one".** The accent marks the current section, a chosen item and a healthy state. It is rarely a fill. The one filled surface in a view is the primary button.
- **The glowing stem means "you are here".** It is a 2 px bar with a soft halo. It marks the current section in the rail (one element that slides from item to item), and it stands before each group and section label.
- **A key is a fine line.** `KeyCut` draws a 28 px line key. Its teeth come from the entry's ID, never from its secret. It stays quiet: it does not carry status colour. Status has its own column.
- **The lock is twelve fine pins.** A new password needs 12 characters. Each character sets one pin on the line. At 12, the line lights.
- **A key wears out.** `LifeRule` shows how much of a key's rotation interval is used, as a row of ticks.
- **A list is ours too.** `FilterSelect` keeps the real select as the field, so its name, value and automation stay the same. A press or a key opens AKM's own list. Focus stays on the field.

## Tokens

Colour comes from a hue `--h` (165) and a chroma factor `--k` (1), at fixed lightness. The dark values are the Odicto values. The light values keep the same relationships. `--warn`, `--danger`, `--moon` and `--logo-stem` do not follow the hue, because they carry meaning.

Measured contrast, worst case over canvas, rail, glass, float and well:

| Text | Light | Dark |
|---|---|---|
| `--text` | 13.75 | 13.08 |
| `--muted` | 6.67 | 7.95 |
| `--faint` | 4.94 | 4.76 |
| `--accent` | 6.70 | 10.70 |
| `--warn` | 5.21 | 9.31 |
| `--danger` | 4.94 | 7.65 |
| `--moon` | 4.60 | 10.08 |
| `--on-action` on `--action-hi` | 6.26 | 4.84 |

## Status

Status is a shape as well as a colour, and words always go with it. A filled dot means fine. A hollow ring means look at this.

## Departures from Odicto

- AKM has light and dark. The theme switch keeps its words and its accessible name.
- A project name keeps its own letter case, because it is the user's data. Only fixed labels are capitals.
- Hover and colour changes are instant, and only `transform` animates (next section). Odicto eases colour over 120 ms.
- A dialog or drawer has no fade.
- At 1040 px and below, the rail is 92 px wide with the icon above the word. The words stay.

## Rules that tests hold

- Colour changes are instant. Only `transform` animates. Press scale is `0.97`, and reduced motion removes it.
- A dialog or drawer is fully opaque from its first frame. It slides; it never fades.
- One filled button per view.
- The current-section marker stays on its item at both rail widths.
- Accessible names are a contract with `tools/verify_windows_app.ps1` and the test suites. Change one only with its test.

## Screens

| Screen | Layout | Notes |
|---|---|---|
| Lock | Stage and form | First run and unlock share it |
| Keys | Project strip above glass groups | Grouped by project |
| Key | Drawer | One secret on screen at a time |
| Add or edit | Drawer | Four labelled groups |
| Use keys | Two columns | Take keys out; bring keys in |
| Recycle Bin, Activity, Settings | One column | |
