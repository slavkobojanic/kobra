# Introduction

Welcome to *The K Programming Language*, the guide to K.

K is a strongly typed language for building full-stack web applications. You write your user
interface, your server, your remote calls and your database access in one language, and the
compiler turns it into plain JavaScript and CSS for the browser and a single self-contained
executable for the server.

K is designed as a successor to Vue, Svelte, React and Angular. It keeps what those frameworks
got right and moves it into the language itself, where the compiler can check it:

- **Components are single declarations**, with their state, view and scoped styles in one place,
  as in a Vue single-file component.
- **Views are declarative and readable**, in the style of SwiftUI, with no XML and no virtual DOM.
- **Reactivity is fine-grained and compiled**, like Svelte 5 and Solid: each piece of state is
  wired directly to the DOM nodes that read it.
- **Services, dependency injection and guards** come from the Java and Spring world, but are
  resolved at compile time.
- **Sum types with exhaustive matching** come from Rust, and **typed, checked errors** keep every
  failure visible.

Here is a complete component:

```k
public component Counter {
    @Prop let label: String
    @State var count: Int = 0

    func increment() {
        count += 1
    }

    view {
        box {
            text { "{label}: {count}" }
                .class(.label)

            button { "Add" }
                .on(click: increment)
        }
    }

    style {
        .label { fontSize: .lg; }
    }
}
```

If you have used Swift, much of this will look familiar: `let` and `var`, `func`, `@State`.
If you have used Vue, the shape of the file will. K borrows ideas, not syntax, and the
[Philosophy](philosophy.md) chapter explains the principles that decide how each idea is adapted.

## Who this book is for

This book is for developers who build web applications, whether you come from a JavaScript
framework, a typed backend language, or both. It assumes you know what HTML, CSS and HTTP are.
It doesn't assume you know Swift, Rust or Spring.

K is also designed for a world where **agents write much of the code and humans review it**. If you
are reviewing K code rather than writing it, the [tour](tour.md) and the chapters on
[components](apps/components.md) and [services](apps/services.md) are the fastest way in.

## How this book is organized

- **Getting started** installs the toolchain, creates a project and walks through the commands you
  use every day. It ends with [a tour of K](tour.md) that shows a small full-stack feature from
  database to button.
- **The language** covers K as a general-purpose language: values, functions, types, control flow,
  errors and modules.
- **Building apps** covers the parts that make K a web language: components, views, styles,
  reactivity, services, routing, data loading, forms, RPC, persistence, authentication, configuration,
  background jobs, email, testing and deployment.
- **Example apps** walks through the six complete apps in the repository's `examples/` directory, from a
  browser-only todo list to a workspace of services.
- **Reference** collects keywords, annotations, operators, commands and project files, every diagnostic
  code with an example and a fix, and the API of every standard-library module.

You can read the book front to back, or jump straight to the chapter you need. Each chapter is
example-driven and stands on its own as far as possible.

## The state of K

K is young, but it runs: the compiler, the server runtime, the test runner, the development server and
the language server are built, and the apps in `examples/` compile, pass their tests and deploy. This
book describes the language as designed and says where the toolchain doesn't follow yet. Two small
markers tell you when something isn't settled:

> **Planned** (k-fmt). The design includes this feature, here the formatter `k fmt`, but the first
> compiler releases don't. It is described so you know where the language is going. The id in parentheses names the
> feature, and a test checks it against the compiler's list of planned features.

> **Open question.** The design records this as undecided. The book explains the options and what holds
> either way.

The [Design status](reference/design-status.md) page lists every planned feature, every open question
and the topics the design has deferred.

## Conventions

Code blocks are K unless they say otherwise. Shell commands start with `k`:

```bash
k dev
```

**Every K sample in this book is compiled** by the book's own test, so a sample can't drift from the
compiler. To keep them short, many samples hide the lines that make them compile, such as imports or
the component around a view fragment; the eye button on a code block shows them. A comment `// …` or
`// ...` marks code left out because it doesn't matter for the point being made. A sample whose first
line names a file in `examples/` is an excerpt of that file, checked against it.
