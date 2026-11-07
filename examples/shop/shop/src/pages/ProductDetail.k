import k.validation.Form
import k.validation.ValidationError
import store.OrderRequest
import store.ProductSummary
import store.StoreService

/// One product, with a form to buy it.
///
/// The URL is named because search engines and shared links depend on it:
/// renaming the page must never move it. The canonical link and the
/// schema.org `Product` data are rendered on the server, so crawlers see them
/// in the first response.
public page ProductDetail at "/products/{slug}" {
    /// The product's slug.
    @Param let slug: String

    @Inject let store: StoreService

    /// The product, loaded before the view renders.
    @Load let product: ProductSummary = try await store.product(slug: slug)

    /// The order form, prefilled with the product and a quantity of one. The
    /// email field starts blank: `@Form` keeps the text typed into each field
    /// and parses the address only when the form validates.
    @Form var order: OrderRequest = .empty(productSlug: slug, quantity: 1)
    @State var problem: String? = nil

    /// Places the order and sends the customer to pay. Validation failures
    /// reported by the server are rethrown, so the form shows them under
    /// their fields. On a network failure the form keeps what was typed, so
    /// the customer can submit again.
    ///
    /// After a timeout the order may already have been placed, and submitting
    /// again places a second one. Neither is charged until the customer pays
    /// on Stripe's page.
    func buy() async throws ValidationError {
        problem = nil

        do {
            let placed = try await store.placeOrder(request: order.value)

            Router.navigate(url: placed.paymentUrl)
        } catch .outOfStock(let available) {
            problem = if available == 0 {
                "Sorry, this just sold out."
            } else {
                "Only {available} left. Lower the quantity and try again."
            }
        } catch .productNotFound {
            problem = "This product is no longer available."
        } catch .paymentUnavailable {
            problem = "Payments are unavailable right now. You haven't been charged; please try again in a minute."
        } catch .rpcFailure(.invalid(let failure)) {
            throw failure
        } catch .rpcFailure(.rejected) {
            problem = "Too many orders from your connection. Please wait a minute and try again."
        } catch .rpcFailure(.offline) {
            problem = "You seem to be offline. Check your connection and try again."
        } catch .rpcFailure(.timeout) {
            problem = "The shop didn't answer in time. You haven't been charged; please try again."
        }
    }

    metadata {
        title("{product.name} · K Shop")
        description(product.summary)
        canonical(.productDetail(slug: slug))
        socialPreview(image: product.imageUrl, title: product.name)
        structuredData(.product(name: product.name, price: product.price))
    }

    view {
        article {
            image(source: product.imageUrl, description: product.name)
                .class(.photo)

            box {
                heading(level: 1) { product.name }

                paragraph { product.price.formatted(as: .currency) }
                    .class(.price)

                paragraph { product.description }

                if product.isInStock {
                    form {
                        numberField(value: bind order.quantity)
                            .label("Quantity")

                        textField(value: bind order.email)
                            .label("Email for your receipt")

                        if let problem {
                            paragraph { problem }
                                .class(.problem)
                                .role(.alert)
                        }

                        button { "Buy now" }
                            .type(.submit)
                            .disabled(order.isSubmitting)
                    }
                    .class(.form)
                    .on(submit: order.handle(buy))
                } else {
                    paragraph { "Sold out" }
                        .class(.soldOut)
                }
            }
            .class(.details)
        }
        .class(.product)
    }

    loading {
        paragraph { "Loading…" }
    }

    failed(let error) {
        match error {
            case .notFound {
                metadata {
                    title("Product not found · K Shop")
                    status(.notFound)
                }

                heading(level: 1) { "Product not found" }

                paragraph { "It may have been renamed or retired." }

                link(to: .home) { "Back to the shop" }
            }

            case .rpcFailure {
                heading(level: 1) { "This product couldn't be loaded" }

                paragraph { "Check your connection and try again." }

                button { "Try again" }
                    .on(click: reload)
            }
        }
    }

    style {
        .product {
            display: grid;
            gridTemplateColumns: 1fr 1fr;
            gap: .lg;

            @media (maxWidth: 720px) { gridTemplateColumns: 1fr; }
        }

        .photo {
            width: 100%;
            borderRadius: .md;
        }

        .details { display: grid; gap: .md; alignContent: start; }
        .price   { fontSize: 1.5rem; fontWeight: 600; }
        .form    { display: grid; gap: .md; maxWidth: 20rem; }
        .problem { color: .danger; }
        .soldOut { color: .danger; fontWeight: 600; }
    }
}
