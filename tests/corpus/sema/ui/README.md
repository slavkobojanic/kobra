# `sema/ui` cases

Test-first cases for components, views, styles, themes and reactivity
([sema.md §12.1–12.5](../../../../docs/compiler/sema.md#12-framework-layer)). Formats are in
[testing.md §3.8](../../../../docs/compiler/testing.md#38-the-sema-suite); codes, spans and the exact
quick-fix wording these cases pin are in
[diagnostics.md](../../../../docs/compiler/diagnostics.md#annotations-members-and-reactivity-k3000k3099)
(K3000–K3399).

| Directory | Rules |
|---|---|
| `annotations/` | `let`/`var` per annotation (K3001), where annotations apply (K3002) |
| `props/` | required named props, chained optional props, set twice, writes to a `@Prop` (K3014) |
| `bindings/` | `bind` places (K3015), missing `bind` (K3016), `bind` where none is expected (K3018) |
| `events/` | `.on(name:)` on elements and components, payload types, one event per call, throwing handlers (K3017) |
| `slots/` | trailing, optional and scoped slots, slot types (K3005) |
| `stylable/` | `@Stylable` props, the parent's classes, unapplied hooks (K3209) |
| `elements/` | the element table, required arguments, HTML names, content rules |
| `attributes/` | per-element attributes and their types, roles |
| `classes/` | class references, `when`, `global(...)`, unused classes |
| `for-key/` | `key` in view loops, its type, `key` outside views |
| `styles/` | properties, value grammar, tokens, `{ }` interpolation, media features, at-rules |
| `keyframes/` | keyframes selectors, content, references, scoping |
| `themes/` | token categories, variants, overrides |
| `reactivity/` | `@Derived`, `@Effect`, `untracked`, `onCleanup` |
| `recovery/` | one mistake, one diagnostic |
| `projects/` | multi-module projects: props, styling and tokens across modules, library tokens, pages |
| `clean/` | the design documents' examples, which must check cleanly |
| `drag/` | `k.drag` (41-drag-and-drop.md): payloads typed by their `DragKind`, `dropped` and `moved` handler types, chained `row` and `accepts` |
