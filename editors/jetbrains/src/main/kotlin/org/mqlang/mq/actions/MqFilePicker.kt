package org.mqlang.mq.actions

import com.intellij.openapi.fileChooser.FileChooser
import com.intellij.openapi.fileChooser.FileChooserDescriptor
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile

private val INPUT_EXTENSIONS = setOf("md", "mdx", "html", "csv", "tsv", "txt")
private val MQ_EXTENSIONS = setOf("mq")

/** Prompts the user to pick a Markdown/MDX/HTML/CSV/TSV/text file to use as mq input, mirroring the VS Code extension's picker. */
fun pickInputFile(project: Project, callback: (VirtualFile) -> Unit) {
    pickFile(project, INPUT_EXTENSIONS, "Select Input File", "Select a .md, .mdx, .html, .csv, .tsv or .txt file as input", callback)
}

/** Prompts the user to pick an `.mq` file, mirroring the VS Code extension's "Execute mq file" picker. */
fun pickMqFile(project: Project, callback: (VirtualFile) -> Unit) {
    pickFile(project, MQ_EXTENSIONS, "Select mq File", "Select a .mq file to execute", callback)
}

private fun pickFile(
    project: Project,
    extensions: Set<String>,
    title: String,
    description: String,
    callback: (VirtualFile) -> Unit,
) {
    val descriptor = FileChooserDescriptor(true, false, false, false, false, false)
        .withTitle(title)
        .withDescription(description)
        .withFileFilter { it.extension?.lowercase() in extensions }
    project.basePath?.let { descriptor.setRoots(com.intellij.openapi.vfs.LocalFileSystem.getInstance().findFileByPath(it)) }

    FileChooser.chooseFile(descriptor, project, null) { file -> callback(file) }
}
