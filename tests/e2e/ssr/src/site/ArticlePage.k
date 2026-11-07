/// An article with two dependent loads, page metadata and a `failed` block
/// that sets the status for some errors and not others.
public page ArticlePage at "/articles/{slug}" {
    @Param let slug: String

    @Inject let articles: Articles

    @Load let article: Article = try await articles.find(slug: slug)
    @Load let related: List<String> = try await articles.related(article: article)

    metadata {
        title("{article.title} · SSR Test")
        description("Read {article.title}")
        canonical(.articlePage(slug: slug))
    }

    view {
        heading(level: 1) { article.title }

        paragraph { article.body }

        for name in related key name {
            paragraph { name }
        }
    }

    loading {
        paragraph { "Loading article" }
    }

    failed(let error) {
        match error {
            case .notFound {
                metadata {
                    title("Not found · SSR Test")
                    status(.notFound)
                }

                heading(level: 1) { "No such article" }
            }

            case .hidden {
                metadata {
                    status(.forbidden)
                }

                paragraph { "This article is hidden." }
            }

            case .locked(let until) {
                paragraph { "Locked until {until}" }
            }
        }
    }
}
