// A component with named arguments and a trailing content block (its unnamed slot).
component T {
    view {
        Card(title: "Your cart", items: items) {
            paragraph { "Review before checkout." }
        }
    }
}
