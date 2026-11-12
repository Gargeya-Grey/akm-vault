# Standard licence texts

`tools/generate_third_party_notices.py` uses these texts only for a shipped package that has no licence file of its own. Each file is the plain-text form of one licence from the [SPDX License List](https://github.com/spdx/license-list-data), version 3.29.0 (`text/<id>.txt` at tag `v3.29.0`), unchanged.

| File | SHA-256 |
|---|---|
| `Apache-2.0.txt` | `074e6e32c86a4c0ef8b3ed25b721ca23aca83df277cd88106ef7177c354615ff` |
| `BSD-3-Clause.txt` | `5a93d5831e1297ab10fe643e1a631e83be392896da14ee2951285a79012df69d` |
| `BSL-1.0.txt` | `84c6ef3ea9e3254a54d0acf5d3e0c61ae011b8fef7dd6940591cf060e6380a8f` |
| `MIT.txt` | `b05785f9f18e6716bab63424b11454513b9943a222595b70411009202fc592b5` |
| `MPL-2.0.txt` | `66a3107d5ad6a058aab753eaac2047ccb2ed0e39465dd0fe5844da3e300d5172` |

When the generator stops with "is missing", add the SPDX text of that identifier from the same version, and its hash here.
