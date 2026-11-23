---
kind: language
design: 07
book: apps/styles.md
---
A package ships a font family with its theme: `Font(family:size:faces:)` lists `FontFace("fonts/Inter.woff2", weight: 700)` files of the package (workspace libraries included), and every app that uses the theme emits their `@font-face` rules, showing the fallback font until each file loads, preloads the body font, and serves the files under content-hashed names.
