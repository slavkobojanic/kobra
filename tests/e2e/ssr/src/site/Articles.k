/// In-memory articles, shared by the server and the browser.
public service Articles {
    /// Finds an article by slug.
    func find(slug: String) async throws ArticleError -> Article {
        if slug == "missing" {
            throw ArticleError.notFound
        }

        if slug == "hidden" {
            throw ArticleError.hidden
        }

        if slug == "locked" {
            throw ArticleError.locked(until: "<tomorrow>")
        }

        if slug == "xss" {
            return Article(slug: slug, title: "<script>alert(1)</script>", body: "</script><script>alert(2)</script>")
        }

        return Article(slug: slug, title: "Article {slug}", body: "Body of {slug} & more")
    }

    /// Related slugs, loaded after the article.
    func related(article: Article) async throws ArticleError -> List<String> {
        return ["{article.slug}-a", "{article.slug}-b"]
    }

    /// A load that panics: a bug, not a domain error.
    func broken() async throws ArticleError -> String {
        let empty: List<String> = []

        return empty[3]
    }
}
