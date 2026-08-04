package org.mqlang.mq.lsp

import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.LanguageServerFactory
import com.redhat.devtools.lsp4ij.client.LanguageClientImpl
import com.redhat.devtools.lsp4ij.server.OSProcessStreamConnectionProvider
import com.redhat.devtools.lsp4ij.server.StreamConnectionProvider

class MqLanguageServerFactory : LanguageServerFactory {
    override fun createConnectionProvider(project: Project): StreamConnectionProvider =
        MqStreamConnectionProvider(project)

    override fun createLanguageClient(project: Project): LanguageClientImpl = LanguageClientImpl(project)
}

private class MqStreamConnectionProvider(project: Project) : OSProcessStreamConnectionProvider() {
    init {
        commandLine = MqLspCommandLine.build(project)
    }
}
