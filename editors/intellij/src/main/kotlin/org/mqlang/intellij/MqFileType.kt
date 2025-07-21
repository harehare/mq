package org.mqlang.intellij

import com.intellij.openapi.fileTypes.LanguageFileType
import javax.swing.Icon

object MqFileType : LanguageFileType(MqLanguage) {
    
    override fun getName(): String = "mq File"
    
    override fun getDescription(): String = "mq language file"
    
    override fun getDefaultExtension(): String = "mq"
    
    override fun getIcon(): Icon? = MqIcons.FILE
}