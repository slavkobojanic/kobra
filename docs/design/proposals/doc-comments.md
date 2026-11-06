# Proposal: Documentation Comments

> **Status: proposal.** Not yet accepted into the design. First used by `examples/weather`.

## Problem

The design requires explicit, reviewable code but doesn't say how declarations are documented. Agents and humans need one obvious way to write API documentation that the language server can show on hover and `k doc` can render.

## Proposal

**Documentation comments start with `///`** and attach to the declaration that follows them. `//` remains an ordinary comment that tooling ignores.

```k
/// Looks up a single place by its GeoNames id.
///
/// - Parameter id: the id from a `Place`, usually taken from the URL.
/// - Returns: the place, from the cache when it has been seen before.
/// - Throws: `WeatherError.placeNotFound` when no place has this id.
public func place(id: Int) async throws WeatherError -> Place { ... }
```

- **The body is Markdown.** The first sentence is the summary shown in autocomplete and in `k doc` indexes. Further paragraphs follow a blank `///` line.
- **Structured sections are Markdown list items, as in Swift:** `- Parameter name:`, or `- Parameters:` with nested `- name:` items for several; `- Returns:`; `- Throws:`. Javadoc's `@param` style isn't used, because `@` already means an annotation in K.
- **Names in backticks that resolve to declarations become links** (`` `WeatherError.placeNotFound` ``, `` `searchPlaces(query:)` ``). The compiler checks them: a reference to a declaration that doesn't exist is a warning, so documentation can't drift after a rename.
- **Fenced code blocks tagged `k` are type-checked** by `k check` as doc examples, but not run.
- **Placement:** before the declaration and before its annotations, so `/// …` comes before `@Inject`, `@Prop` and the rest.
- **Required where it's API:** `k check` warns on a `public` declaration without a doc comment in a package (see [18-packages.md](../18-packages.md)). In an application it's a lint, not an error.
- **What gets documented:** every `public` type, function, property and enum case. Annotated component and page fields (`@Prop`, `@Param`, `@Slot`, and so on) are the declaration's API too, so they're documented the same way.

## Why

- `///` is already familiar from Swift and Rust, which K draws on.
- Markdown sections avoid a second annotation-like syntax inside comments.
- Checked references follow the "compile time over runtime" principle: documentation is code that can break.
