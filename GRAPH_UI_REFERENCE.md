# Mittens Graph UI Reference

## Layout Contract
- Two-step interaction only:
- Step 1: select renderer from left list.
- Step 2: click backend from right list to open `renderer_port/?backend_id=<id>` immediately.
- No auto-navigation from graph page.

## Visual Language
- Dark monochrome base (`#050505` to `#151515`) with subtle static noise.
- High-contrast text with muted secondary metadata.
- Flat panels and flat rows.
- No glossy cards.
- No animated gradients.
- No motion-heavy hover effects.

## Interaction Style
- Selection is explicit via border and `[selected]` label.
- Backend rows are disabled until renderer is selected.
- Focus backend (query `focus_backend`) is marked, never auto-opened.

## System Data Presentation
- Keep operational data first:
- renderer id, port, worktree, live count.
- backend id, project, branch, listeners, ws url.
- service health and live edges remain visible but secondary.

## Keep It Clean
- Prefer compact monospace typography.
- Use minimal color accents only for health state (`up/down`).
- Avoid decorative UI trends unless explicitly requested.
