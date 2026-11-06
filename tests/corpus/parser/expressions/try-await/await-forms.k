// `await` before calls, members and chains
func f() {
    await refresh()
    let a = await api.load().items
}
