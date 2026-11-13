# ECAD's interface and sheet fonts

ECAD embeds two font families in its binary (brief 037, owner answer Q1, 2026-11-13: the fonts
live in ECAD's repository, not in `storytold/craft-fonts`; offering them there is a later,
owner-approved step). `ecad-ui` includes the bytes with `include_bytes!` on native and wasm, so
the faces are always present and nothing is read from disk at run time.

| Family | Use | Licence |
| --- | --- | --- |
| IBM Plex Sans | All interface text (egui family `ui`, and the first face of `Proportional`) | SIL Open Font License 1.1 (`OFL-1.1`), `ibm-plex-sans/OFL.txt` |
| Barlow Semi Condensed | Page text and reports (egui family `sheet`; the sheet switches to it in U4a) | SIL Open Font License 1.1 (`OFL-1.1`), `barlow-semi-condensed/OFL.txt` |

Every file is **unmodified** from its official source: the font files byte for byte, and each
family's licence text as published (IBM ships Plex's licence as `LICENSE.txt`; it is stored here
as `OFL.txt` with its content unchanged, CRLF line ends included). `.gitattributes` in this
folder keeps git from converting them. A test in `ecad-ui` (`fonts_embedded`) checks that the
SHA-256 of every file below equals the committed bytes.

Do not edit, subset or re-save these files. To update a font, download the new release from the
same source, replace the file, and update its row (version, URL, SHA-256, size).

## Sources

**IBM Plex Sans**: font version 3.005, from the IBM Plex release
[`@ibm/plex-sans@1.1.0`](https://github.com/IBM/plex/releases/tag/%40ibm%2Fplex-sans%401.1.0),
asset `ibm-plex-sans.zip`
(<https://github.com/IBM/plex/releases/download/%40ibm/plex-sans%401.1.0/ibm-plex-sans.zip>),
files taken from `ibm-plex-sans/fonts/complete/ttf/` and `ibm-plex-sans/LICENSE.txt`.
Copyright © 2017 IBM Corp., with Reserved Font Name "Plex".

**Barlow Semi Condensed**: font version 1.408, from
[`google/fonts` `ofl/barlowsemicondensed`](https://github.com/google/fonts/tree/3218ae2acc5bce7ccd97c477965b5b4094241fb0/ofl/barlowsemicondensed)
at commit `3218ae2acc5bce7ccd97c477965b5b4094241fb0` (the latest commit touching that folder on
2026-11-13), each file from
`https://raw.githubusercontent.com/google/fonts/3218ae2acc5bce7ccd97c477965b5b4094241fb0/ofl/barlowsemicondensed/<file>`.
Copyright 2017 The Barlow Project Authors (<https://github.com/jpt/barlow>).

| File | Version | Bytes | SHA-256 |
| --- | --- | --- | --- |
| `ibm-plex-sans/IBMPlexSans-Regular.ttf` | 3.005 | 200500 | `975dcda37d80f038dcd143c22e33ca2d97a0cc5a929aace1c749153b0fe1afa5` |
| `ibm-plex-sans/IBMPlexSans-Medium.ttf` | 3.005 | 202460 | `331c8639d7598b2cde62a911a71db195e30cb655cd6bdf2e324a7e984955f907` |
| `ibm-plex-sans/IBMPlexSans-SemiBold.ttf` | 3.005 | 202632 | `a20caf8286023a6a7a85e40b1d2a4ae9fc3e3b1f9eda8f4c542dd4986af67bb1` |
| `ibm-plex-sans/OFL.txt` | plex-sans 1.1.0 | 4456 | `7e6b2818edbd8f6a01ae80641cc8f16a51080d08fb4e532be3a0b6f74adb07da` |
| `barlow-semi-condensed/BarlowSemiCondensed-Regular.ttf` | 1.408 | 105776 | `e8c7242f612b13d026e18a94b63b0fd079e91a3faf009d1b6981dbb5f16d1153` |
| `barlow-semi-condensed/BarlowSemiCondensed-SemiBold.ttf` | 1.408 | 109772 | `bd299f4bc5b44d30be8d42e9bad3a5df7d66af1cd55d0ed72a8b8916360a1424` |
| `barlow-semi-condensed/OFL.txt` | google/fonts `3218ae2a` | 4377 | `186d750eb496a4c17a76385f82be6aea2ac1cf2de074a811d63786cf374ea73f` |

## Measured size

The five faces add **821 140 bytes (0.78 MiB)** of font data to the binary: IBM Plex Sans
605 592 bytes (three faces), Barlow Semi Condensed 215 548 bytes (two faces).
