// A page's `description(…)` is kept in the browser like its title
// (22-metadata.md): `$k.description(() => text)` keeps the document's
// `<meta name="description">` on the innermost mounted page, so a
// client-rendered app's pages, whose shell the server wrote without them,
// and client navigation both show the page's own (todos-G). The server
// still writes it into the head it renders.
page Notes {
    metadata {
        title("Notes")
        description("Everything you wrote down.")
    }

    view {
        text { "notes" }
    }
}
