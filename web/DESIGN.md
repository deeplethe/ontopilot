# The interface, in six rules

Utopia's chrome is neutral dark glass: Geist for text, Marcellus for the wordmark, no hue in the chrome, colour reserved for data and for three semantic states. That language is written down in `web/src/styles.css` and has been since the first screen. What was missing was enforcement — a page could pick any of twelve pixel sizes, any of fourteen paddings, any grey. These rules close that gap. They are checked by `pnpm guard` in CI; a page that breaks them does not merge.

## 1. Five type sizes, by name

| name | size / line | for |
|---|---|---|
| `text-fine` | 11 / 16 | metadata, chip text, table headers, hints under a control |
| `text-small` | 12 / 18 | secondary text, dense rows, captions |
| `text-body` | 13 / 20 | everything else: prose, controls, menus |
| `text-title` | 15 / 22 | section and dialog titles |
| `text-display` | 20 / 28 | the page title, and only that |

No `text-xs`/`text-sm`, no `text-[11px]`. If a size between two steps seems necessary, the step is wrong for the element, not the scale for the size. Weight is `font-medium` for controls and titles, `font-semibold` only on the primary button; `font-bold` is not used in chrome. Numbers in chrome are Geist with `u-num` (tabular figures), never monospace; `font-mono` is for keys, ids, code and URLs.

## 2. Six spacing steps

`1 2 3 4 6 8` (4, 8, 12, 16, 24, 32 px), for padding, margin and gap alike. No half steps, no pixels. Values of `12` and above are layout, not rhythm — clearance under a floating bar, a footer's breathing room — and are allowed for that. Controls carry their own padding — a page never sets padding on a button or an input. Page gutters are `6` or `8`; the gap between two related controls is `2`; between two groups, `4`; between two sections, `6`.

## 3. Four radii, named by role

A corner is `rounded-cell` (4 px), `rounded-control` (6 px), `rounded-panel` (8 px) or `rounded-overlay` (12 px), and which one it is follows from what the thing is: a chip, a table cell, a `kbd`, a small icon target is a cell; a button, an input, a select, a segmented group is a control; a card, a list, a dialog body, a code block is a panel; a menu, a popover, a toast, a floating dock — anything that hovers over the page — is an overlay. `rounded-full` only on things that are actually circles: an avatar, a status dot, a colour swatch, a graph node.

The names are the point. `rounded-panel` says what the box is, the way `text-ink-2` says what the grey is for, and that is what the guard can check — a number cannot be wrong, only a role can. Four rather than one because the same absolute radius is not the same roundness at every size: 8 px reads as generously rounded on a 24 px chip and as nearly square on a 300 px panel. Nested corners go inwards, never outwards: a control (6) inside a panel (8), a panel inside an overlay (12).

## 4. Colour is a token, never a value

Text is `text-ink` or `text-ink-2` — two levels: the content, and what is said about the content. There is no third, fainter level; a caption, a timestamp or a placeholder is already marked as secondary by where it sits and how big it is, and dimming it again only makes it harder to read. Lines are `border-line` and `border-line-strong`. Fills are `bg-surface` (rest), `bg-surface-2` (hover), `bg-surface-3` (selected). Meaning is `ok`, `warn`, `danger`, `contest`, `violet`, and those five appear only where they mean something — a status, a contested edge, a destructive action — never as decoration. `neutral-500`, `white/10`, `rose-400`, `[var(--u-…)]` do not appear in a page; the tokens are defined once in `styles.css` and exposed as Tailwind colours, and that is the only door.

Glass is a surface treatment, not a colour: `glass` for a panel in peripheral vision, `glass-strong` for one being read, and both go solid under the pointer (see the note above `--u-surface-strong-hover`). A page does not write `backdrop-blur`.

## 5. State lives in the component

Hover, focus, active, disabled and motion are defined once, in `web/src/ui/`, and a page never writes `hover:`, `focus:`, `transition` or `duration-`. Every control shows a visible focus ring for keyboard users (`--u-ring`); every disabled control is `opacity-40` with `cursor-not-allowed`; every hover settles in `--u-fast` (120 ms) and leaves in `--u-base` (260 ms). A page that needs a control that does not exist adds it to `ui/`, with all five states, and then uses it.

Concretely, a page renders no raw `<button>`, `<input>`, `<textarea>` or `<select>`; it renders `Button`, `IconButton`, `Input`, `Textarea`, `NativeSelect`, `Dropdown`, `SearchSelect`. Confirmation is `DangerConfirm` or `Dialog`, never `window.confirm`. A hint on hover is `Tooltip`, not a bare `title=` on a span (a `title` on a button that already has a visible label is fine).

## 6. A panel is a slot for content

The first five rules say what a panel looks like. This one says when there is one.

A panel holds **several things of the same kind** — the rows of a table, the items of a list. A group of form fields, a block of prose, the only content in a page's main region: no panel. The page is already their container, and a border, a fill and a radius each claim "this is an object separate from its surroundings" — spent on a single object, they say nothing and flatten the hierarchy of everything around them.

A list is **one panel with rows**, not one card per item. Cards per item put seven or eight boxes on a page at the same level, and each card ends up being both the panel and the clickable thing — which is how a slot acquires a hover state it has no business having. With rows, hover belongs to the row (`hover:bg-surface-2`, already the pattern in `ui/table.tsx`) and the panel never responds to the pointer.

Controls that operate on a panel's contents — filter, search, sort, pagination — sit **outside** it, in the page header or above it. They are not content, and when a filter empties the list the panel has to become an empty state without taking the only way to change the filter with it.

The other thing a panel may be is **the reach of one action**: a settings card (`SettingsCard`) whose footer holds the Save that applies to exactly what the border encloses, and nothing else. The border earns its keep by answering "what does this button send?" — so a page of them is a page of small independent saves, not one long form with a single button at the bottom that quietly ships every field on the screen. A page with only one such card does not need it; the page is already the boundary.

Settings and other read-a-column-of-fields pages are centred and width-limited (`mx-auto max-w-3xl`), not stretched to the window.

Exempt: the floating panels on Graph and Ontology. Those are `glass-strong` surfaces over a canvas, and their job is to hold the canvas down so they can be read — a different problem from this one.

## How this is enforced

`web/scripts/style-guard.mjs` scans `web/src/**/*.tsx` for the patterns above and fails CI on any hit. It runs first in `pnpm build`. While the pages were being migrated, `web/style-guard.baseline.json` listed the ones not yet done; every page passes now and the file is gone. A new file is checked from its first commit.

## Migration order

By weight of `className` sites: Ontology, Graph, Library, Review, Settings; then the rest. A migration PR changes classes and swaps raw controls for components, and touches no logic — that is what makes it reviewable by diff alone.
