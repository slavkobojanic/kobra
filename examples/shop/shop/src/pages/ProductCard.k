import store.ProductSummary

/// A product tile for the storefront grid, linking to the product page.
public component ProductCard {
    /// The product to show.
    @Prop let product: ProductSummary

    view {
        article {
            link(to: .productDetail(slug: product.slug)) {
                image(source: product.imageUrl, description: product.name)
                    .class(.photo)
                    .loading(.lazy)

                heading(level: 2) { product.name }
                    .class(.name)
            }
            .class(.link)

            paragraph { product.summary }
                .class(.summary)

            if product.isInStock {
                paragraph { product.price.formatted(as: .currency) }
                    .class(.price)
            } else {
                paragraph { "Sold out" }
                    .class(.soldOut)
            }
        }
        .class(.card)
    }

    style {
        .card {
            display: grid;
            gap: .sm;
            padding: .md;
            borderRadius: .md;
            backgroundColor: .surfaceSunken;
        }

        .link {
            color: .text;
            textDecoration: none;

            &:hover .name, &:focus-visible .name { color: .primary; }
        }

        .photo {
            width: 100%;
            aspectRatio: 1;
            objectFit: cover;
            borderRadius: .md;
        }

        .name { fontSize: 1.125rem; }
        .summary { color: .muted; }
        .price { fontWeight: 600; }
        .soldOut { color: .danger; fontWeight: 600; }
    }
}
