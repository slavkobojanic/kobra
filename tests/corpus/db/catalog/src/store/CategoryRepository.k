/// Categories.
public repository CategoryRepository for Category {
    /// Every category in display order.
    func ranked() async -> List<Category> {
        return await categories.sortedBy { category => category.rank }
    }

    /// The featured ones: a Bool field as the whole condition.
    func featured() async -> List<Category> {
        return await categories.filter { category => category.featured }
    }
}
