package org.mqlang.mq.icons

import com.intellij.ide.FileIconProvider
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import javax.swing.Icon

/**
 * Gives `.mq` files their own icon in the project tree, tabs and elsewhere.
 *
 * `.mq` syntax highlighting is registered via a TextMate bundle rather than a dedicated [com.intellij.openapi.fileTypes.FileType]
 * (see [org.mqlang.mq.textmate.MqTextMateBundleProvider]), which leaves files with the generic TextMate icon.
 * [FileIconProvider] overrides the icon by file name alone, without needing to own the file type.
 */
class MqFileIconProvider : FileIconProvider {
    override fun getIcon(file: VirtualFile, flags: Int, project: Project?): Icon? =
        if (file.extension == "mq") MqIcons.FILE else null
}
