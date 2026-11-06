# Documentation Comments

## Decision

**Documentation comments start with `///`** and attach to the declaration that follows them. `//` remains an ordinary comment that tooling ignores. The body is Markdown, and the language server shows it on hover and `k doc` renders it.

```k
/// Looks up a single place by its GeoNames id.
///
/// - Parameter id: the id from a [`Place`], usually taken from the URL.
/// - Returns: the place, from the cache when it has been seen before.
/// - Throws: [`WeatherError.placeNotFound`] when no place has this id.
func place(id: Int) async throws WeatherError -> Place { ... }
```

## Rules

- **The body is Markdown.** The first sentence is the summary shown in autocomplete and in `k doc` indexes. Further paragraphs follow a blank `///` line.
- **Structured sections are Markdown list items, as in Swift:** `- Parameter name:`, or `- Parameters:` with nested `- name:` items for several; `- Returns:`; `- Throws:`. Javadoc's `@param` style isn't used, because `@` already means an annotation in K.
- **Links to declarations are written as intra-doc links, as in rustdoc:** a code span in square brackets, `` [`Name`] `` or `` [`Type.member`] ``. They render as links, and the compiler checks them: a link that doesn't resolve is a warning, so documentation can't drift after a rename.
- **What a link may name:** an identifier (`` [`Place`] ``), a dotted path of identifiers (`` [`WeatherError.offline`] ``, `` [`k.http.HttpStatus`] ``), or a function name with its labels (`` [`searchPlaces(query:)`] ``).
- **Plain backticks are code, never links,** and are never checked: `` `300ms` ``, `` `"todos"` ``, `` `nil` ``, `` `a + b` ``, and also names such as `` `App.k` `` or `` `temperature_unit` `` that aren't K declarations. A name is checked only when the author asks for a link.
- **A link resolves against, in order:** the members and parameters of the declaration the comment documents (`` [`id`] ``, `` [`items`] ``), the file's imports, the file's own package (an application counts as one package, so a declaration in another module of the same app resolves), and fully qualified names (`` [`k.http.HttpStatus`] ``). A link that resolves against none of these is a warning.
- **Fenced code blocks tagged `k` are type-checked** by `k check` as doc examples, but not run.
- **Placement:** before the declaration and before its annotations, so `/// …` comes before `@Prop`, `@Param` and the rest. Block doc comments (`/** */`) don't exist.

### What must be documented

Documentation is required where it is API:

- **Public top-level declarations,** and their **non-private functions, computed properties and enum cases.**
- **The annotated inputs of components and pages:** `@Prop`, `@Binding`, `@Slot`, `@Emits`, `@Stylable`, `@Param`. They are the declaration's API, like a function's parameters.
- **Stored fields of public structs, entities and configs are a lint only,** because their names and types usually say enough.
- **Injected fields are never required,** since they are always private ([16-modules.md](16-modules.md)).

In a package ([18-packages.md](18-packages.md)), `k check` warns on missing documentation for this API. In an application it is a lint.

## Why

- `///` is already familiar from Swift and Rust, which K draws on.
- Markdown sections avoid a second annotation-like syntax inside comments.
- Checked references follow "compile time over runtime": documentation is code that can break.
