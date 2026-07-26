# Branding assets

The project uses a two-level logo system:

- `assets/branding/aeroplane-chess-icon.svg`: vector master for the square app mark.
- `assets/branding/aeroplane-chess-icon-round.svg`: circular-mask export for Android round launcher icons.
- `assets/branding/aeroplane-chess-lockup.svg`: horizontal mark plus wordmark for menus, web headers, and promotional material.
- `assets/branding/aeroplane-chess-logo.png`: generated 1024px raster export for the existing game asset slot.

## Usage rules

- Keep clear space around the icon equal to the dark outer frame thickness, approximately 40 units in the 1024-unit master.
- Use the simplified icon at 16–48px. Do not add extra shadows, glows, or textures at export time.
- Use the lockup when the product name must be readable. Do not recreate the wordmark with a different font or stretch the icon independently.
- Prefer the color mark on a neutral light or dark background with sufficient contrast. The four player colors are part of the product identity, but they must not be the only way to communicate gameplay state.
- Do not skew, stretch, rotate, crop, or place the mark on a visually busy background.

## Export

Run `./scripts/generate-branding-assets.sh` after changing the icon SVG. The script exports the Web/PWA sizes, favicon ICO, and Android density buckets from the same vector master so platform icons remain consistent.

The current source intentionally avoids small grid lines, bevels, realistic reflections, and deep shadows. Those details belong in promotional artwork, not in the core mark.

The SVG is the canonical source of truth. The aircraft's right wing and right star badge are horizontally mirrored from the left-side geometry, so future edits should change the left-side source group first rather than maintaining two independent coordinate sets. Any PNG preview or platform icon should be regenerated from the SVG and should not be edited separately.

The central aircraft stays inside the central circle, and the nearest route dot is kept outside the circle's stroke so the circle cannot cover it. Route arrows point inward: top down, bottom up, left right, and right left.
