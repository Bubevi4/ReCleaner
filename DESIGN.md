# Recleaner DESIGN.md

## Product
Windows cleaner + gaming optimizer desktop utility (Rust + Slint). Audience: power users and gamers who want clear risk levels before deleting caches or applying tweaks.

## Visual language
Dark utility console: dense lists, fixed columns, quiet chrome. Not a marketing site. Identity comes from the green accent on dark charcoal panels and the Cleanup row grid (checkbox → name → risk badge → metrics).

## Dial
`ENERGY 1 / RHYTHM 1 / MOTION 1`

Calm, uniform list rhythm; motion limited to progress bars and hover on controls.

## Palette
| Token | Value | Role |
|-------|-------|------|
| bg | `#0f1419` | Window |
| bg-sidebar | `#0b1015` | Nav rail |
| bg-elevated | `#171d25` | Rows / panels |
| border | `#2a3441` | Hairlines |
| text | `#e8eef5` | Primary |
| text-muted | `#8b9bb0` | Secondary |
| accent | `#3ecf8e` | Primary actions, selected language, brand wordmark |
| safe / moderate / risky | `#3ecf8e` / `#e6b84d` / `#e85d5d` | Risk badges only |

One accent (green). Risk colors are semantic, not decorative accents.

## Typography
Segoe UI / Segoe UI Semibold: matches Windows shell, readable at 11–14px in dense rows. Page titles 22px / 700.

## Layout rules
- Content packs to the top; leftover height goes into a scroll area or an explicit stretch sink (never equal-gap stretch between labels).
- Cleanup row columns are the alignment reference for list screens.
- Settings uses a single compact group (language + path + safety note), not full-bleed oversized controls.
- Border radius 6–8px on panels/buttons; 4px on badges. No pills.

## Copy
Specific verbs (Analyze, Clean, Apply selected). No em dashes. Risk warnings stay short and concrete.
