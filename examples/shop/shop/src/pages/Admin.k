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
    @State var problem: String? = nil

    /// Signs out. The page requires a signed-in admin, so as soon as the
    /// response says the session has ended, the router leaves the page for
    /// the server, which sends the browser to the sign-in page; nothing
    /// after the `await` runs. If the server can't be reached, the session
    /// is still open, so the page stays and says so.
    func signOut() async {
        problem = nil

        do {
            try await admin.signOut()
        } catch _ {
            problem = "You couldn't be signed out. Check your connection and try again."
        }
    }

    metadata {
        title("Orders · K Shop admin")
    }

    view {
        header {
            heading(level: 1) { "Orders" }
                .class(.title)

            button { "Refresh" }
                .on(click: reload)

            button { "Sign out" }
                .on(click: signOut)
        }
        .class(.toolbar)

        if let problem {
            paragraph { problem }
                .class(.problem)
                .role(.alert)
        }

        if orders.isEmpty {
            paragraph { "No orders yet." }
        } else {
            table {
                row {
                    headerCell { "Placed" }
                        .class(.cell)

                    headerCell { "Item" }
                        .class(.cell)

                    headerCell { "Customer" }
                        .class(.cell)

                    headerCell { "Total" }
                        .class(.cell)

                    headerCell { "Status" }
                        .class(.cell)
                }

                for order in orders key order.id {
                    row {
                        cell { order.placedAt.formatted(as: .dateTime) }
                            .class(.cell)

                        cell { "{order.quantity} × {order.productName}" }
                            .class(.cell)

                        cell { "{order.email}" }
                            .class(.cell)

                        cell { order.total.formatted(as: .currency) }
                            .class(.cell)

                        cell { order.status.label }
                            .class([.cell, .status, .attention when order.status == .needsReview])
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
            case .rpcFailure(.rejected) {
                paragraph { "Your session has ended. Sign in again to continue." }

                link(to: .signIn.returnTo(.admin)) { "Sign in" }
            }

            default {
                paragraph { "The orders couldn't be loaded. Check your connection and try again." }

                button { "Try again" }
                    .on(click: reload)
            }
        }
    }

    style {
        .toolbar {
            display: flex;
            gap: .sm;
            alignItems: center;

            & .title { marginRight: auto; }
        }

        .orders {
            width: 100%;
            borderCollapse: collapse;

            & .cell {
                padding: .sm;
                borderBottom: 1px solid .border;
                textAlign: left;
            }
        }

        .status { fontWeight: 600; }
        .attention { color: .danger; }
        .problem { color: .danger; }
    }
}
