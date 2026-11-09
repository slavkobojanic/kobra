# Tutorial: build Shelf

In this tutorial you'll build Shelf, a reading list: the books you want to read, and the ones you've
read. You'll start with a page that says hello and end with a full-stack app, with accounts so each person
keeps their own shelf, the books saved in a database, tests, and one file to run in production.

Each step is one short page that adds one thing to the app, and ends with the app working. Read them in
order: a step uses only what the steps before it explained. If you haven't built anything with K yet, the
[Quick start](../quick-start.md) is a shorter first look, and you need the `k` command
([Installation](../../getting-started/installation.md)).

## The steps

| Step | Page | You add | You learn |
|---|---|---|---|
| 1 | [Create the project](01-create.md) | `k new shelf`, the layout, `k dev` | projects, pages, `view`, `k check` |
| 2 | [Show a list of books](02-list.md) | a `Book` struct, a list of books, a `BookRow` component | `struct`, `List`, `for` in views, `@Prop` |
| 3 | [Add books with a form](03-form.md) | the add-book form, its messages, the first tests | `@State`, `Validatable`, constraint annotations, `@Form`, `@Emits`, tests |
| 4 | [Make it look right](04-style.md) | a theme, classes, a dark variant | `theme`, tokens, scoped styles, variants |
| 5 | [Save books on the server](05-server.md) | a database table, a protocol, a service | `entity`, `repository`, migrations, `protocol`, `rpc`, `service … implements`, `do` and `catch` |
| 6 | [Load books from the server](06-load.md) | loading and failure states, reloading after adding | `@Load`, `loading` and `failed`, `match` on typed errors, `reload()` |
| 7 | [A page per book](07-routing.md) | `/books/{id}`, with notes and a rating, a second migration | `@Param`, `at`, `link(to:)`, `routes.lock`, errors of your own |
| 8 | [Accounts](08-accounts.md) | sign up, sign in, each person's own shelf | `k.auth`, sessions, `requires`, guards, `@SignInPage` |
| 9 | [Test it](09-testing.md) | tests of the failure states, the server's rules, the forms and the whole server | fakes, `use … for …`, `testing` modules, `expect throws`, `TestClient` |
| 10 | [Ship it](10-ship.md) | a setting, `k build`, `dist/app` with its own database | `config`, `@Env`, secrets, `schema.lock`, deployment |

All ten steps are written.

## Following along

Every step's finished project is in the repository, under `examples/learn/shelf/`: `01-create`,
`02-list` and so on. Each page shows every file that changed since the step before, so you can type the
changes yourself, or copy a step's directory and start from there. Each page also has **Try it**
sections: a small change to make, often a mistake on purpose, and what K says about it.

You need a terminal and an editor. Keep `k dev` running in one terminal while you work, and run
`k check` in a second one.

## Recap

- Shelf is built in ten steps, each one page that ends with a working app.
- Each step uses only what earlier steps explained.
- The finished project of every step is in `examples/learn/shelf/`.

## Next

- [1. Create the project](01-create.md).
