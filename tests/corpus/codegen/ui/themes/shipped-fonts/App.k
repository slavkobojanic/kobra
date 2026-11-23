// A theme's `FontFace`s ship their files (07-themes-and-units.md, "Shipped
// fonts"). The `brand` library below `.deps/brand/` is what `k` mounts for a
// workspace library: its faces name its own files. Each face is one
// `@font-face` with `font-display: swap` before `:root`, at a content-hashed
// URL below `/assets/fonts/`, and faces of one family, weight and style
// dedupe. The document preloads one face: the body font's upright face
// nearest weight 400.
public app Wedded {}
