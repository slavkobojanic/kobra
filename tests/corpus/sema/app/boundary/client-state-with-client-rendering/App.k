// With client rendering, a page may read a client-only service's state.
@Rendering(.client)
public app Todos {
    metadata {
        title("Todos")
    }
}
