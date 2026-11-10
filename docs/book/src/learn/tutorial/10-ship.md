# 10. Ship it

In this step you'll run Shelf the way it runs in production: one file, configured from its environment,
with its own database. You'll add a setting someone running Shelf can change without a new build, build
the app, and start the server it makes.

Start from the project you finished in [step 9](09-testing.md). The finished step is in the repository as
`examples/learn/shelf/10-ship`.

## A setting of your own

Whoever runs a copy of Shelf for a family or a book club wants to stop strangers making accounts once
everyone is in. That's configuration: a value that differs between copies of the same build. In K it's a
`config` declaration, whose fields come from environment variables. Create
`src/accounts/ShelfConfig.k`:

```k
// examples/learn/shelf/10-ship/src/accounts/ShelfConfig.k
/// The settings someone running Shelf can change without a new build.
public config ShelfConfig {
    /// Whether new people can make an account. Set it to `false` once
    /// everyone who should have a shelf has one.
    @Env("SHELF_SIGN_UPS") let signUpsOpen: Bool = true
}
```

- `@Env("SHELF_SIGN_UPS")` names the variable. The name is written out, never made from the field's
  name, so renaming the field can't break a deployment, and a search for `SHELF_SIGN_UPS` finds the code.
- The default makes it optional. A field without one is required, and the server refuses to start
  without it.
- A config is server-only, and is read and checked once, before the server answers anything.

Inject it like a service, in `src/accounts/AuthApi.k`:

```k
// examples/learn/shelf/10-ship/src/accounts/AuthApi.k
// ...
    @Inject let clock: Clock
    @Inject let config: ShelfConfig

    rpc signUp(credentials: Credentials) async throws SignUpError {
        guard config.signUpsOpen else {
            throw SignUpError.closed
        }

// ...
```

Give the error its new case, in `src/accounts/SignUpError.k`, and say so in `src/accounts/AuthProtocol.k`:

```k
// examples/learn/shelf/10-ship/src/accounts/SignUpError.k
// ...
    /// Shelf isn't taking new accounts (`SHELF_SIGN_UPS` is `false`).
    case closed
// ...
```

```k
// examples/learn/shelf/10-ship/src/accounts/AuthProtocol.k
// ...
    /// - Throws: `SignUpError.emailTaken` when the address is taken, and
    ///   `.closed` when Shelf isn't taking new accounts.
// ...
```

Then show it on the sign-up page, `src/accounts/SignUp.k`. Until you do, `k check` reports that the
`do` in `submit` doesn't handle every error (`K2206`), naming `.closed`:

```k
// examples/learn/shelf/10-ship/src/accounts/SignUp.k
// ...
        } catch .emailTaken {
            problem = "There's already an account with this email address."
        } catch .closed {
            problem = "Shelf isn't taking new accounts."
// ...
```

To try it with `k dev`, put the variable in a file named `.env` in the project, which `k dev` reads and
`.gitignore` leaves out of the repository:

```text
SHELF_SIGN_UPS=false
```

Save it: `k dev` rebuilds when `.env` changes, as it does for code. Sign out and try to make an account,
then delete the file. A project commits an
`.env.example` instead, listing what a deployment sets, with placeholders rather than real values:

```text
# examples/learn/shelf/10-ship/.env.example
# What a deployment of Shelf sets; `dist/app config` lists every variable.
# Copy this file to .env to try a setting with `k dev`. Never commit .env.
SHELF_SIGN_UPS=true
# DATABASE_URL=postgres://shelf@db.internal:5432/shelf
# DATABASE_PASSWORD_FILE=/run/secrets/database-password
```

## Configs in tests

Tests never read the environment, so a test whose code reaches a config says which value to use. The
sign-up and sign-in tests reach `ShelfConfig` through `AuthApi`. Add a `use` rule to each group, in
`src/accounts/SignUp.test.k` and `src/accounts/SignIn.test.k`:

```k
// examples/learn/shelf/10-ship/src/accounts/SignIn.test.k
// ...
test SignIn {
    use ShelfConfig(signUpsOpen: true) for ShelfConfig
// ...
```

In a test, a config has an initializer with a parameter per field, so the value is written in the rule.
Then test the closed case, with a group of its own at the end of `SignUp.test.k`:

```k
// examples/learn/shelf/10-ship/src/accounts/SignUp.test.k
// ...
test SignUp {
    use ShelfConfig(signUpsOpen: true) for ShelfConfig
// ...
        expect session.user == nil
    }
}

test SignUpClosed {
    use ShelfConfig(signUpsOpen: false) for ShelfConfig

    @Inject let session: Session

    test "no one can make an account while sign-ups are closed" {
        let screen = render(SignUp())

        screen.textField(labeled: "Email").fill("ada@example.com")
        screen.passwordField(labeled: "Password").fill("correct horse")
        screen.button(named: "Make an account").click()
        await screen.settle()

        expect screen.alert().text == "Shelf isn't taking new accounts."
        expect session.user == nil
    }
}
```

```console
$ k check
checked 37 files, ran 24 tests: no problems
```

## Build

`k check` passing is what makes a build worth shipping: it fails on any error, any failing test, and any
lockfile that doesn't match the code. Then build:

```console
$ k build
wrote rpc.lock
built dist
```

`rpc.lock` changed because `SignUpError` has a new case. That's a change a browser tab still running the
old code can take, so `k check` allows it; removing a case wouldn't be.

`dist/app` is the whole app in one file: the server, a JavaScript runtime to run it on, every file the
browser loads, and every migration. Copying it to a server is a complete deployment. It needs no `k`,
no Node.js and no source code. `dist/app config` lists every variable it reads:

```console
$ dist/app config
DATABASE_PASSWORD  Secret<String>?  optional  secret
DATABASE_URL       Url              required
K_PUBLIC_ORIGIN    Url              optional
K_SESSION_IDLE     Duration         optional  default "7d"
K_SESSION_MAX_AGE  Duration         optional  default "30d"
K_SHUTDOWN_GRACE   Duration         optional  default "25s"
K_TRUSTED_PROXIES  List<String>     optional  default ""
K_WORKER_HEAP_MB   Int              optional  default "256"
PORT               Int              optional  default "8080"
SHELF_SIGN_UPS     Bool             optional  default "true"
```

Most come from K's own configs: `DATABASE_URL` from the database's, `K_SESSION_IDLE` from the sessions'.
`DATABASE_PASSWORD` is a **secret**, a `Secret<String>`: the server never prints it, not in this list
and not in a log, and the browser never gets it. A secret can also come from a file, with
`DATABASE_PASSWORD_FILE`, which is how Docker and Kubernetes hand secrets to a program.

## Run it

`k dev` made a database for you, and filled in what was missing. `dist/app` doesn't: production settings
are never guessed. Start it with nothing set:

```console
$ dist/app
error: configuration is invalid (1 problem)
  DATABASE_URL  missing (required by k.db.DatabaseConfig.url; set DATABASE_URL)
```

It exits with status 78 before it opens a port, so a deployment with a missing setting fails at once,
rather than at the first request that needs it. In production, `DATABASE_URL` names a Postgres database;
to try the build on your own computer, a SQLite file works too. Set the port and the address people
reach the app at as well:

```console
$ DATABASE_URL=sqlite:shelf.db PORT=5310 K_PUBLIC_ORIGIN=http://localhost:5310 dist/app
{"level":"info","message":"applied migration 0001_create_book_records"}
{"level":"info","message":"applied migration 0002_add_notes_and_ratings"}
{"level":"info","message":"applied migration 0003_add_accounts"}
shelf serving on port 5310
```

The new database got every migration, in order, before the server opened its port. A database that
already has some gets only the ones it hasn't; that's what `schema.lock`'s list of migrations is for.
Open `http://localhost:5310`: it's the same Shelf, now as it runs in production. Ctrl-C stops it, after
it finishes the requests it's answering.

`K_PUBLIC_ORIGIN` matters once Shelf is on the internet: with an `https://` address, the session cookie
is sent only over HTTPS. `dist/app probe http://127.0.0.1:5310/sign-in` exits with status 0 when the page
answers, for a container's health check.

**Try it:** set a value that isn't a `Bool`:

```console
$ DATABASE_URL=sqlite:shelf.db SHELF_SIGN_UPS=maybe dist/app
error: configuration is invalid (1 problem)
  SHELF_SIGN_UPS  "maybe" is not a Bool (expected true or false)
```

Every value is parsed into its field's type at startup, and every problem is listed at once.

**Try it:** change history. A migration that has run on some database is history: changing it would
leave that database different from a new one. In `migrations/0001_create_book_records.k`, rename the
`author` column to `writer`, and run `k check`:

```text
error[K8005]: schema.lock doesn't record the table `book_record` as the migrations leave it
  --> schema.lock:18:5
   |
18 |     { field = "author", name = "author", type = "text" },
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = note: the migrations create the column `writer`, which schema.lock doesn't record
   = note: schema.lock records the column `author`, which the migrations don't create
   = help: run `k build`, which records the schema the migrations produce in schema.lock, and review the change; if the lock was right, a migration changed after it was recorded

checked 37 files: 1 error(s), 0 warning(s)
```

`schema.lock` records what the migrations produce, so a changed migration can't reach production
unnoticed. Change it back; a rename belongs in a new migration.

## What you built

Shelf is finished: a reading list with accounts, where each person's books are saved on the server,
listed, rated and noted on a page of their own, tested from the form to the server, and shipped as one
file that checks its configuration and migrates its database before it serves.

## Recap

In this step you:

- declared a `config` with an `@Env` variable and a default, and injected it;
- bound a config in tests with `use … for …`;
- built the app into `dist/app` with `k build`, and listed its settings with `dist/app config`;
- ran it with its own database, which it migrated before serving;
- saw `schema.lock` refuse a migration that changed after it was recorded.

## Where next

- [Configuration and secrets](../../apps/config.md) covers `config`, `Secret`, `.env` and `[dev]`
  settings in full.
- [Deployment](../../apps/deployment.md) covers `dist/app`'s commands, Postgres, proxies, containers and
  several servers.
- [A tour of K](../../tour.md) is a fast pass over the whole language, and [The basics](../../language/basics.md)
  starts the language guide.
- The [example apps](../../examples/todos.md) are finished apps to read, from a todo list to a shop.
