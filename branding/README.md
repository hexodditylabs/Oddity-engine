# Oddity Engine branding

Oddity Engine is made by **Hex0ddity Labs**. The full brand board is
`oddity-engine-brand.html` (open it in a browser).

## Files

| File | Use |
| --- | --- |
| `oddity-cat.svg` | The wizard cat, 15×15 grid. Window icon, toolbar, sizes up to 64 px. |
| `oddity-cat-striped.svg` | The cat with stripes, 16×16 grid. 128 px and larger only. |
| `oddity-cat-light.svg` | The cat for light backgrounds (pale pink tile). |
| `oddity-wordmark.svg` | ODDITY wordmark with the cursor block. |
| `oddity-wordmark-striped.svg` | Wordmark with stripes, for large display. |
| `hex0ddity-mark.svg` | Hex0ddity Labs mark. |

All are pixel art on a whole-pixel grid (`shape-rendering="crispEdges"`).
Scale them by whole numbers when you can, so pixels stay square.

In code, these are available as `oddity_icons::Icon::oddity_cat()` and friends.

## Colours

| Name | Hex | Use |
| --- | --- | --- |
| Coral | `#FF5A5F` | Cat face, wordmark |
| Hot pink | `#FF2D55` | Accent: selection, focus, primary buttons, cursor block |
| Pale pink | `#FFE4E1` | Hat, stripes, light tile |
| Black | `#000000` | Logo backgrounds |
| Ink | `#0A0A0A` | Near-black panels |

## Type

| Font | Use |
| --- | --- |
| Space Grotesk (Medium, Bold) | Brand titles: Hub, splash, about |
| Geist Mono (Regular, Medium) | Labels in caps (ENGINE), code, Output |
| Inter (Regular, SemiBold) | Editor UI text, as in Godot |

All three are under the SIL Open Font License and are bundled in
`crates/oddity_widgets/assets/fonts/`.
