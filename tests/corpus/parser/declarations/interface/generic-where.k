// A generic interface with a constraint.
interface Repository<T> where T implements Identifiable {
    func find(id: T.Id) async -> T?
    func all() async -> List<T>
}
