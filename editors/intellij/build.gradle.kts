// The Kobra plugin for JetBrains IDEs (editors/intellij/README.md).
//
// Highlighting, brackets and comments come from the VS Code extension's
// TextMate grammar and language configuration, copied into the plugin as a
// TextMate bundle at build time (`textmateBundle`): editors/vscode is their
// one source, and nothing of them is committed here. Everything else comes
// from `k lsp` through the IntelliJ Platform's LSP API.

import org.jetbrains.intellij.platform.gradle.TestFrameworkType
import org.jetbrains.intellij.platform.gradle.tasks.PrepareSandboxTask

plugins {
    id("org.jetbrains.kotlin.jvm") version "2.3.21"
    id("org.jetbrains.intellij.platform") version "2.20.1"
}

group = "slavkobojanic"
version = providers.gradleProperty("pluginVersion").get()

repositories {
    mavenCentral()
    intellijPlatform {
        defaultRepositories()
    }
}

dependencies {
    intellijPlatform {
        intellijIdea(providers.gradleProperty("platformVersion"))
        bundledPlugin("org.jetbrains.plugins.textmate")
        bundledModule("intellij.platform.lsp")
        testFramework(TestFrameworkType.Platform)
        pluginVerifier()
    }
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.opentest4j:opentest4j:1.3.0")
}

// 2026.2 runs on Java 25 (its product-info.json's minRequiredJavaVersion).
kotlin {
    jvmToolchain(25)
}

intellijPlatform {
    pluginConfiguration {
        id = "slavkobojanic.kobra"
        name = "Kobra"
        version = providers.gradleProperty("pluginVersion")
        ideaVersion {
            sinceBuild = providers.gradleProperty("sinceBuild")
            untilBuild = provider { null }
        }
        vendor {
            name = "Slavko Bojanic"
            url = "https://github.com/slavkobojanic/kobra"
        }
    }
    // The verifier checks the plugin against the IDE it is built with, so
    // it needs no download beyond that one.
    pluginVerification {
        ides {
            create(
                org.jetbrains.intellij.platform.gradle.IntelliJPlatformType.IntellijIdea,
                providers.gradleProperty("platformVersion").get(),
            )
        }
    }
}

// The TextMate bundle: the VS Code extension's manifest (which names the
// grammar and the language configuration), grammar and language
// configuration, unchanged. crates/k_lsp/tests/editors.rs checks that the
// plugin takes them from editors/vscode and keeps no copy of its own.
val textmateBundle = tasks.register<Sync>("textmateBundle") {
    from(layout.projectDirectory.dir("../vscode")) {
        include("package.json", "language-configuration.json", "syntaxes/**")
    }
    into(layout.buildDirectory.dir("textmate/kobra"))
}

// The bundle sits next to the plugin's jars (`<plugin>/textmate/kobra`), in
// the distribution and in the sandboxes the IDE and the tests run from:
// TextMate reads a bundle from a directory, not from inside a jar.
tasks.withType<PrepareSandboxTask>().configureEach {
    from(textmateBundle) {
        into(pluginName.map { "$it/textmate/kobra" })
    }
    // The licence: MIT OR Apache-2.0, like the rest of the repository.
    from(layout.projectDirectory.dir("../..")) {
        include("LICENSE-MIT", "LICENSE-APACHE")
        into(pluginName)
    }
}

tasks.test {
    useJUnit()
}
