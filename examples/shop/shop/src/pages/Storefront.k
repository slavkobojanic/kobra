import cart.CartBadge

/// The shell around every page: the shop's header with the cart, the page
/// content, and a short footer.
///
/// `App.k` applies this layout to every page. Its `metadata` entries are the
/// defaults that each page overrides.
public layout Storefront {
    /// The current page.
    @Slot let content: View

    metadata {
        title("K Shop")
        description("Small-batch goods for people who write K.")
    }

    view {
        header {
            link(to: .home) { "K Shop" }
                .class(.brand)

            CartBadge()
        }
        .class(.header)

        main { content }
            .class(.main)

        footer {
            paragraph { "Payments are handled by paybox. Prices include VAT." }
        }
        .class(.footer)
    }

    style {
        .header {
            display: flex;
            justifyContent: spaceBetween;
            alignItems: center;
            padding: .md .lg;
            borderBottom: 1px solid .border;
        }

        .brand {
            fontSize: 1.25rem;
            fontWeight: 700;
            color: .text;
            textDecoration: none;
        }

        .main {
            maxWidth: 64rem;
            margin: 0 auto;
            padding: .lg;
        }

        .footer {
            padding: .lg;
            fontSize: 0.875rem;
            color: .muted;
            textAlign: center;
        }
    }
}
