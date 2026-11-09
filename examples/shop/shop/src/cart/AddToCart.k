import store.ProductSummary

/// "Add to cart" on a product page. The cart lives in the browser, so the
/// button appears after hydration.
@Rendering(.client)
public component AddToCart {
    /// The product to add.
    @Prop let product: ProductSummary

    @Inject let cart: CartService

    view {
        button { "Add to cart" }
            .on(click: { cart.add(CartItem(slug: product.slug, name: product.name)) })
            .class(.add)
    }

    loading {
    }

    style {
        .add {
            justifySelf: start;
            padding: .sm .md;
            borderRadius: .md;
        }
    }
}
