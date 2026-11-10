import checkout.Cart
import ui.ThemeToggle

/// The shop's one page: pays for a mug through paybox.
public page Home at "/" {
    @Inject let cart: Cart

    metadata {
        title("Stack shop")
    }

    view {
        main {
            ThemeToggle()

            heading(level: 1) { "K mug, $9.90" }

            paragraph { cart.message }

            button { "Pay" }
                .on(click: { await cart.pay() })
                .class(.pay)
        }
        .class(.page)
    }

    style {
        .page {
            display: grid;
            gap: .md;
            maxWidth: 32rem;
            margin: 0 auto;
            padding: .lg;
            backgroundColor: .surface;
            color: .text;
        }

        .pay {
            justifySelf: start;
            padding: .sm .md;
            border: none;
            borderRadius: .md;
            backgroundColor: .primary;
            color: .onPrimary;
        }
    }
}
