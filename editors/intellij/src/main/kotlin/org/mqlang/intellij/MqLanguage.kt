package org.mqlang.intellij

import com.intellij.lang.Language

object MqLanguage : Language("mq") {
    override fun getDisplayName(): String = "mq"
    
    override fun isCaseSensitive(): Boolean = true
    
    override fun getMimeTypes(): Array<String> = arrayOf("text/mq")
}