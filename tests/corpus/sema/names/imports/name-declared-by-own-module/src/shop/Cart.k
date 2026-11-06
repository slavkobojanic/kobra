// Importing a name the file's module already declares is K1106 at the import.
import catalog.Item

func describe(item: Item) -> Int {
    return 0
}
