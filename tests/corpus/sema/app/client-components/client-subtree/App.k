// A server-rendered app with one client-rendered component: inside its subtree, client-only
// services and interfaces are fine (no K4012, no K4013).
public app Todos {
    metadata {
        title("Todos")
    }
}
