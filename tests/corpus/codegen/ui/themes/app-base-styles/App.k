// An app's stylesheet starts with a small base stylesheet drawn from its theme
// (07-themes-and-units.md, "Base styles"): the body has no margin and takes the
// `font body` token and the page colours (`text`, `background`), form controls
// inherit the font, and `color-scheme` follows each variant (dark for a variant
// whose page background is dark, so native controls and scrollbars match).
// The server knows the variants, so a document carries the remembered one's
// classes on `<html>` from the first byte.
public app Themed {}
