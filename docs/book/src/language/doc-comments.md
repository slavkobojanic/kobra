# Documentation comments

Documentation comments start with `///` and document the declaration that follows them. `//` is an
ordinary comment that tools ignore. The body is Markdown; the language server shows it on hover and in
completion.

<!-- The declarations the samples in this chapter use; checked, not rendered.

```k
// src/weather/Context.k
struct Place {
    let id: Int
    let name: String
}

enum WeatherError implements Error {
    case placeNotFound(id: Int)
    case offline
}
```
-->

```k
/// Looks up a single place by its GeoNames id.
///
/// - Parameter id: the id from a [`Place`], usually taken from the URL.
/// - Returns: the place, from the cache when it has been seen before.
/// - Throws: [`WeatherError.placeNotFound`] when no place has this id.
func place(id: Int) async throws WeatherError -> Place {
    // …
~    throw .placeNotFound(id: id)
}
```

## The body

- **The first sentence is the summary,** shown in completion lists and on hover. Further paragraphs
  follow a blank `///` line.
- **Structured sections are Markdown list items**, as in Swift: `- Parameter name:`, or `- Parameters:`
  with a nested `- name:` item for each; `- Returns:`; `- Throws:`. There is no Javadoc `@param`,
  because `@` already means an annotation.
- **Placement:** before the declaration and before its annotations, so `///` comes above `@Prop`,
  `@Param` and the rest. There are no block doc comments (`/** */`).

```k
/// A card that shows one note.
component NoteCard {
    /// The note to show.
    @Prop let note: Place

    /// Fired when the person asks to open the note.
    @Emits let opened: Event<Void>

    view {
        button { note.name }
            .on(click: { opened.emit() })
    }
}
```

## Links to declarations

**A link is a code span in square brackets**, as in rustdoc: `` [`Place`] ``,
`` [`WeatherError.offline`] ``, `` [`k.http.HttpStatus`] ``, or a function with its labels,
`` [`place(id:)`] ``. `k check` and the editor resolve every link, and one that names nothing is
`K1901`, a warning, so documentation can't drift after a rename. Go to definition, find references and
rename treat a link like any other reference.

A link resolves against, in order:

1. the members and parameters of the declaration the comment documents (`` [`id`] ``);
2. the file's imports;
3. every declaration of the file's package (an app counts as one package);
4. the core module, and fully qualified names (`` [`k.http.HttpStatus`] ``).

**Plain backticks are code, never links,** and are never checked: `` `300ms` ``, `` `nil` ``,
`` `App.k` ``, `` `a + b` ``. A name is checked only when you ask for a link.

```k,fails=K1901
/// Saves the draft. See [`Draft.revision`], which doesn't exist.
func save() {}
```

The `fails=K1901` sample is a warning, not an error, so `k check` still passes with it; it is shown so
you know what the warning looks like.

## What should be documented

Documentation belongs wherever something is API:

- public top-level declarations, and their non-private functions, computed properties and enum cases;
- the annotated inputs of components and pages: `@Prop`, `@Binding`, `@Slot`, `@Emits`, `@Stylable` and
  `@Param`, which are the declaration's API as a function's parameters are;
- stored fields of public structs, entities and configs, where the name and type don't already say
  enough. Injected fields are always private and never need documentation.

> **Planned** (doc-coverage, doc-blocks, k-doc). The design has `k check` warn about undocumented API in a package (and offer it as a lint
> in an app), type-check fenced `k` blocks inside doc comments, and render the comments with `k doc`.
> None of these is built: today the compiler checks links (`K1901`) and the language server shows the
> comments.

The standard library follows these rules throughout; its [reference](../reference/stdlib/k-core.md) is
generated from its doc comments.
