package slavkobojanic.kobra

import com.intellij.openapi.util.SystemInfo
import com.intellij.util.EnvironmentUtil
import java.io.File
import java.nio.file.Files
import java.nio.file.InvalidPathException
import java.nio.file.Path

/** Finding the `k` executable that runs the language server. */
object KobraToolchain {
    /** The install one-liner of the book's Installation page (the private repository, through the GitHub CLI). */
    const val INSTALL_SH =
        "gh api repos/slavkobojanic/kobra/contents/scripts/install.sh -H 'Accept: application/vnd.github.raw' | sh"

    /** The same for Windows PowerShell. */
    const val INSTALL_PS1 =
        "gh api repos/slavkobojanic/kobra/contents/scripts/install.ps1 -H 'Accept: application/vnd.github.raw' | Out-String | iex"

    /** Where a toolchain installation lives, and the environment that moves it. */
    data class Environment(
        val path: String?,
        val home: String?,
        val kHome: String?,
        val windows: Boolean,
    ) {
        companion object {
            /** The IDE's: its `PATH` is the login shell's, also when it was started from the Dock. */
            fun current() = Environment(
                path = EnvironmentUtil.getValue("PATH"),
                home = System.getProperty("user.home"),
                kHome = EnvironmentUtil.getValue("K_HOME"),
                windows = SystemInfo.isWindows,
            )
        }
    }

    private fun isExecutable(path: Path): Boolean = Files.isRegularFile(path) && Files.isExecutable(path)

    private fun pathOf(text: String): Path? = try {
        Path.of(text)
    } catch (_: InvalidPathException) {
        null
    }

    /**
     * The `k` to run: the configured one (a path, or a bare name looked up on
     * `PATH`) or, with none configured, `k` on `PATH`, then the installer's
     * `$K_HOME/bin/k` (`~/.k/bin/k`). `null` when there is none.
     */
    fun find(
        configured: String,
        env: Environment = Environment.current(),
        executable: (Path) -> Boolean = ::isExecutable,
    ): Path? {
        val name = if (env.windows) "k.exe" else "k"
        val setting = configured.trim()
        if (setting.isNotEmpty()) {
            val expanded = if ((setting == "~" || setting.startsWith("~/")) && env.home != null) {
                env.home + setting.substring(1)
            } else {
                setting
            }
            val bare = !expanded.contains('/') && !expanded.contains('\\')
            if (!bare) {
                return pathOf(expanded)?.takeIf(executable)
            }
            return onPath(expanded, env, executable)
                ?: (if (env.windows && !expanded.endsWith(".exe")) onPath("$expanded.exe", env, executable) else null)
        }
        val home = env.kHome?.takeIf { it.isNotBlank() } ?: env.home?.let { "$it${File.separator}.k" }
        return onPath(name, env, executable)
            ?: home?.let { pathOf(it) }?.resolve("bin")?.resolve(name)?.takeIf(executable)
    }

    private fun onPath(name: String, env: Environment, executable: (Path) -> Boolean): Path? {
        val separator = if (env.windows) ';' else ':'
        return env.path.orEmpty()
            .split(separator)
            .filter { it.isNotBlank() }
            .firstNotNullOfOrNull { dir -> pathOf(dir)?.resolve(name)?.takeIf(executable) }
    }
}
