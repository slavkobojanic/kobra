# 9. Test it

In this step you'll test the parts of Shelf the earlier steps left untested: what the start page shows
when the server can't be reached, the rules the server enforces on its own, the sign-up and sign-in
forms, and what the server sends for a page before any of the app's code runs in the browser. No code
of the app changes; every file this step adds or changes is a test.

Start from the project you finished in [step 8](08-accounts.md). The finished step is in the repository
as `examples/learn/shelf/09-testing`. You don't need `k dev` running for this one: tests run in
`k check`, or on their own with `k test`.

## What a test can reach

Since step 3 your tests have used the app's real code all the way down. A test is one browser talking to
one server, in one process:

- `render(…)` shows a component or page as the browser would, and `screen` finds things on it the way a
  person using a screen reader would: by role, label and text.
- `@Inject` in a test group gives the test the app's services. A protocol it injects is the browser's
  side, so its calls go through the wire format, validation and guards to the server's code, as they do
  in production.
- Every test gets its own empty database, its own signed-out session and its own services, so tests can't
  disturb each other.

That covers a lot, but not everything. In a test the server is always there: no call fails with
`.offline`, so step 6's `failed` block has never run in a test. For that, a test swaps in a **fake**.

## Fake the network

A fake is a stand-in for one of the app's services, written for tests. Create a directory named
`src/testing/`: a module of that name is **test-only**, so tests can import it, and nothing in it ever
reaches a build. Add `src/testing/OfflineBookService.k`:

```k
// examples/learn/shelf/09-testing/src/testing/OfflineBookService.k
import books.Book
import books.BookDraft
import books.BookReview
import books.BookService
import books.LookupError
import k.crypto.Uuid
import k.rpc.RpcOnlyError

/// A stand-in for `BookService` that can't reach the server until
/// `reconnect()`, as if the browser were offline. This module is
/// test-only: only tests can use it.
public service OfflineBookService implements BookService {
    private var isOnline: Bool = false

    /// Makes later calls succeed, with an empty shelf.
    func reconnect() {
        isOnline = true
    }

    func all() async throws RpcOnlyError -> List<Book> {
        guard isOnline else {
            throw RpcOnlyError.rpcFailure(.offline)
        }

        return []
    }

    func get(id: Uuid) async throws LookupError -> Book {
        throw LookupError.rpcFailure(.offline)
    }

    func add(draft: BookDraft) async throws RpcOnlyError -> Book {
        throw RpcOnlyError.rpcFailure(.offline)
    }

    func review(book: Book, review: BookReview) async throws LookupError -> Book {
        throw LookupError.rpcFailure(.offline)
    }
}
```

- `implements BookService` names a service, not an interface. That's allowed only in test-only code, and
  the fake must then have every function `BookService` has, with the same types.
- The fake throws `.rpcFailure(.offline)`, the error the browser's runtime throws when the request
  doesn't get out. It fakes `BookService` rather than `BookProtocol` for that reason: a protocol's
  handler throws only its own cases, and `rpcFailure` is never one of them.

Use it in a second test group at the end of `src/home/Home.test.k`, after importing it:

```k
// examples/learn/shelf/09-testing/src/home/Home.test.k
// ...
import testing.OfflineBookService
// ...

test HomeOffline {
    use OfflineBookService for BookService

    @Inject let shelf: OfflineBookService
    @Inject let sessions: SessionManager

    test "the page says when the server can't be reached" {
        await sessions.signIn(userId: AccountId(value: Uuid.random()))
        let screen = render(Home())
        await screen.settle()

        expect screen.text("Shelf can't reach the server. Check your connection.").isVisible
        expect screen.button(named: "Try again").isVisible
    }

    test "trying again loads the books once the server answers" {
        await sessions.signIn(userId: AccountId(value: Uuid.random()))
        let screen = render(Home())
        await screen.settle()

        shelf.reconnect()
        screen.button(named: "Try again").click()
        await screen.settle()

        expect screen.text("No books yet. Add the first one below.").isVisible
    }
}
```

- `use OfflineBookService for BookService` says: in this group, wherever the app injects a
  `BookService`, give it the fake. The page doesn't know.
- After that `use`, `@Inject let shelf: OfflineBookService` gets the same fake the page has, so the test
  can call `reconnect()` on it.
- A file can hold several groups. Each group has its own `use` rules, so the tests of `Home` above this
  one keep the real service.

**Try it:** leave a function out of the fake. Delete `review` from `OfflineBookService`:

```text
warning[K1105]: `books.BookReview` is imported but never used
 --> src/testing/OfflineBookService.k:3:8
  |
3 | import books.BookReview
  |        ^^^^^^^^^^^^^^^^
  |
  = fix: Remove the import

error[K2010]: `OfflineBookService` is missing `review`
  --> src/testing/OfflineBookService.k:12:46
   |
12 | public service OfflineBookService implements BookService {
   |                                              ^^^^^^^^^^^

checked 36 files: 1 error(s), 1 warning(s)
```

A fake can't fall behind the service it stands in for: when `BookService` gains a function, every fake
of it stops compiling until it has one too. Put `review` back.

## Test the server's rules

Some rules only the server can keep: that nobody signed out gets in, that one person can't reach
another's books, and that a request that skips the browser's form is still checked. Test them through
the protocol, as the browser calls it, in `src/books/BookApi.test.k`:

```k
// examples/learn/shelf/09-testing/src/books/BookApi.test.k
import accounts.AccountId
import k.auth.SessionManager
import k.crypto.Uuid
import k.rpc.RpcOnlyError

test BookApi {
    @Inject let api: BookProtocol
    @Inject let sessions: SessionManager

    test "every rpc needs someone signed in" {
        expect throws RpcOnlyError.rpcFailure(.rejected(.unauthenticated)) {
            try await api.list()
        }

        expect throws LookupError.rpcFailure(.rejected(.unauthenticated)) {
            try await api.get(id: Uuid.random())
        }
    }

    test "another person's book isn't found" {
        await sessions.signIn(userId: AccountId(value: Uuid.random()))
        let book = try await api.add(draft: BookDraft(title: "Piranesi", author: "Susanna Clarke"))

        await sessions.signIn(userId: AccountId(value: Uuid.random()))

        expect throws LookupError.notFound {
            try await api.get(id: book.id)
        }

        expect throws LookupError.notFound {
            try await api.review(id: book.id, review: BookReview(notes: "Mine now", rating: 1))
        }
    }

    test "the server checks a review's rules too" {
        await sessions.signIn(userId: AccountId(value: Uuid.random()))
        let book = try await api.add(draft: BookDraft(title: "Piranesi", author: "Susanna Clarke"))

        expect throws LookupError.rpcFailure(.invalid) {
            try await api.review(id: book.id, review: BookReview(notes: "", rating: 6))
        }
    }
}
```

- `expect throws` passes when the block throws an error that matches, and fails otherwise. A pattern can
  stop early: `.rpcFailure(.invalid)` matches every validation failure, whatever field it names.
- `.rejected(.unauthenticated)` is the `Authenticated` guard refusing the call before `BookApi` runs.
- The last test sends a rating of 6, which the review form would never let through. The server runs
  `BookReview`'s rules again, and refuses it.

**Try it:** break a test on purpose. Change the `6` to `3` and run `k check`:

```text
error[K7510]: expected an error matching LookupError.rpcFailure(.invalid), but nothing was thrown
  --> src/books/BookApi.test.k:39:23
   |
39 |         expect throws LookupError.rpcFailure(.invalid) {
   |                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = note: in test `BookApi › the server checks a review's rules too`

checked 36 files, ran 23 tests: 1 error(s), 0 warning(s)
```

A failed test is reported like any other error, with the line and the test's full name. Change it back.

## Test the forms

The sign-up and sign-in pages are tested through their forms, with real accounts. `Session`, injected in
the test, is the browser's view of who's signed in. Add `src/accounts/SignUp.test.k`:

```k
// examples/learn/shelf/09-testing/src/accounts/SignUp.test.k
import k.auth.Session
import k.test.render
import k.text.EmailAddress

test SignUp {
    @Inject let auth: AuthService
    @Inject let session: Session

    test "making an account signs it in" {
        let screen = render(SignUp())

        screen.textField(labeled: "Email").fill("ada@example.com")
        screen.passwordField(labeled: "Password").fill("correct horse")
        screen.button(named: "Make an account").click()
        await screen.settle()

        expect session.user != nil
    }

    test "an email address has only one account" {
        try await auth.signUp(credentials: Credentials(email: EmailAddress("ada@example.com"), password: "correct horse"))
        try await auth.signOut()

        let screen = render(SignUp())

        screen.textField(labeled: "Email").fill("Ada@example.com")
        screen.passwordField(labeled: "Password").fill("another horse")
        screen.button(named: "Make an account").click()
        await screen.settle()

        expect screen.alert().text == "There's already an account with this email address."
        expect session.user == nil
    }

    test "a password needs eight characters" {
        let screen = render(SignUp())

        screen.textField(labeled: "Email").fill("ada@example.com")
        screen.passwordField(labeled: "Password").fill("short")
        screen.button(named: "Make an account").click()
        await screen.settle()

        expect screen.text("Use at least 8 characters").isVisible
        expect session.user == nil
    }
}
```

The second test makes the account through `AuthService` first, then tries again in another letter case,
which `AuthApi` treats as the same address. Then `src/accounts/SignIn.test.k`:

```k
// examples/learn/shelf/09-testing/src/accounts/SignIn.test.k
import k.auth.Session
import k.test.render
import k.text.EmailAddress

test SignIn {
    @Inject let auth: AuthService
    @Inject let session: Session

    test "the right password signs in" {
        try await auth.signUp(credentials: Credentials(email: EmailAddress("ada@example.com"), password: "correct horse"))
        try await auth.signOut()

        let screen = render(SignIn())

        screen.textField(labeled: "Email").fill("ada@example.com")
        screen.passwordField(labeled: "Password").fill("correct horse")
        screen.button(named: "Sign in").click()
        await screen.settle()

        expect session.user != nil
    }

    test "a wrong password doesn't" {
        try await auth.signUp(credentials: Credentials(email: EmailAddress("ada@example.com"), password: "correct horse"))
        try await auth.signOut()

        let screen = render(SignIn())

        screen.textField(labeled: "Email").fill("ada@example.com")
        screen.passwordField(labeled: "Password").fill("wrong horse")
        screen.button(named: "Sign in").click()
        await screen.settle()

        expect screen.alert().text == "That email address and password don't match an account."
        expect session.user == nil
    }
}
```

## Test what the server sends

`render` shows a page in the browser. Some things happen before that, on the server: the redirect to
sign in, the `404` for a missing book, the `<title>` a search engine reads. `TestClient` sends a request
through the whole server, as a browser opening a URL does, and returns the response. Add a group to the
end of `src/books/BookDetails.test.k`, with its imports at the top:

```k
// examples/learn/shelf/09-testing/src/books/BookDetails.test.k
import accounts.AccountId
import k.auth.SessionManager
import k.crypto.Uuid
import k.http.HttpRequest
import k.http.HttpResponse
import k.server.PublicUrls
import k.test.TestClient
import k.test.render
// ...

test BookDetailsOverHttp {
    @Inject let client: TestClient
    @Inject let urls: PublicUrls
    @Inject let sessions: SessionManager
    @Inject let shelf: BookService

    /// Asks the server for the page of the book with this id, as a browser
    /// opening the URL would.
    func open(id: Uuid) async -> HttpResponse {
        return await client.send(request: HttpRequest(method: .get, url: urls.absolute(.bookDetails(id: id))))
    }

    test "someone signed out is sent to sign in, and back afterwards" {
        let id = Uuid.random()

        let response = await open(id: id)

        expect response.status == .seeOther
        expect response.headers["location"] == "{Page.signIn.returnTo(.bookDetails(id: id)).url}"
    }

    test "the page arrives with the book's title" {
        await sessions.signIn(userId: AccountId(value: Uuid.random()))
        let book = try await shelf.add(draft: BookDraft(title: "Piranesi", author: "Susanna Clarke"))

        let response = await open(id: book.id)

        expect response.status == .ok
        expect response.body.contains("<title>Piranesi · Shelf</title>")
    }

    test "a book that doesn't exist answers 404" {
        await sessions.signIn(userId: AccountId(value: Uuid.random()))

        let response = await open(id: Uuid.random())

        expect response.status == .notFound
        expect response.body.contains("<title>Book not found · Shelf</title>")
    }
}
```

- A test group can have helper functions, such as `open(id:)`, for what its tests share.
- `urls.absolute(.bookDetails(id:))` builds the page's full URL from its `Page` case, so the test doesn't
  write the URL either.
- The client acts as the test's one browser, so `sessions.signIn` signs its requests in.

## Run them

```console
$ k check
checked 36 files, ran 23 tests: no problems
```

`k test` runs only the tests, and names each one. `--filter` runs the tests whose name contains some
text:

```console
$ k test --filter HomeOffline
running 2 tests
PASS  HomeOffline › the page says when the server can't be reached
PASS  HomeOffline › trying again loads the books once the server answers

test result: ok. 2 passed; 0 failed; 21 filtered out
```

While you work on a part of the app, `k test --watch` reruns the tests a change affects every time you
save.

## Recap

In this step you:

- swapped a service for a fake in one test group with `use … for …`, in a test-only `testing` module;
- tested the `failed` block, and trying again, with the fake;
- tested the server's guards and rules through the protocol, with `expect throws`;
- tested the sign-up and sign-in forms with real accounts;
- tested redirects, status codes and titles through the whole server with `TestClient`.

## Next

- [10. Ship it](10-ship.md): build Shelf, configure it, and run it as it runs in production.
- [Testing](../../apps/testing.md) covers fakes, test doubles, time and `TestClient` in full.
