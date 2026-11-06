import admin.AdminService
import k.auth.Authenticated
import k.auth.Role
import store.OrderSummary

/// The order dashboard for staff.
///
/// The guards run on the server before anything loads: a visitor without a
/// session is redirected to `SignIn`, and a signed-in user who isn't an
/// admin gets `403 Forbidden`. The rpc behind the page carries the same
/// guards, so the data is protected even if someone calls it directly.
public page Admin requires Authenticated, Role(.admin) {
    @Inject let admin: AdminService

    /// The 50 most recent orders, newest first.
    @Load let orders: List<OrderSummary> = try await admin.recentOrders()

    func signOut() async {
        await admin.signOut()

        Router.navigate(to: .home)
    }

    metadata {
        title("Orders · K Shop admin")
    }

    view {
        header {
            heading(level: 1) { "Orders" }

            button { "Refresh" }
                .on(click: reload)

            button { "Sign out" }
                .on(click: signOut)
        }
        .class(.toolbar)

        if orders.isEmpty {
            paragraph { "No orders yet." }
        } else {
            table {
                row {
                    headerCell { "Placed" }

                    headerCell { "Item" }

                    headerCell { "Customer" }

                    headerCell { "Total" }

                    headerCell { "Status" }
                }

                for order in orders key order.id {
                    row {
                        cell { order.placedAt.formatted(as: .dateTime) }

                        cell { "{order.quantity} × {order.productName}" }

                        cell { "{order.email}" }

                        cell { order.total.formatted(as: .currency) }

                        cell { order.status.label }
                            .class([.status, .attention when order.status == .needsReview])
                    }
                }
            }
            .class(.orders)
        }
    }

    loading {
        paragraph { "Loading orders…" }
    }

    failed(let error) {
        match error {
            case .rpcFailure {
                paragraph { "Your session has ended. Sign in again to continue." }

                link(to: .signIn.returnTo(.admin)) { "Sign in" }
            }
        }
    }

    style {
        .toolbar {
            display: flex;
            gap: .sm;
            alignItems: center;

            & h1 { marginRight: auto; }
        }

        .orders {
            width: 100%;
            borderCollapse: collapse;

            & th, & td {
                padding: .sm;
                borderBottom: 1px solid .border;
                textAlign: left;
            }
        }

        .status    { fontWeight: 600; }
        .attention { color: .danger; }
    }
}
