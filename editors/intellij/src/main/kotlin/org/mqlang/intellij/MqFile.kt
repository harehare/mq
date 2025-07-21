package org.mqlang.intellij

import com.intellij.extapi.psi.PsiFileBase
import com.intellij.openapi.fileTypes.FileType
import com.intellij.psi.FileViewProvider

class MqFile(viewProvider: FileViewProvider) : PsiFileBase(viewProvider, MqLanguage) {
    
    override fun getFileType(): FileType = MqFileType
    
    override fun toString(): String = "mq File"
}