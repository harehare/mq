package org.mqlang.intellij.settings

import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.components.PersistentStateComponent
import com.intellij.openapi.components.Service
import com.intellij.openapi.components.State
import com.intellij.openapi.components.Storage
import com.intellij.util.xmlb.XmlSerializerUtil

@Service
@State(
    name = "org.mqlang.intellij.settings.MqSettings",
    storages = [Storage("MqSettings.xml")]
)
class MqSettings : PersistentStateComponent<MqSettings> {
    
    var mqPath: String = ""
    var showExamplesInNewFile: Boolean = true
    var enableLsp: Boolean = true
    
    companion object {
        fun getInstance(): MqSettings {
            return ApplicationManager.getApplication().getService(MqSettings::class.java)
        }
    }
    
    override fun getState(): MqSettings = this
    
    override fun loadState(state: MqSettings) {
        XmlSerializerUtil.copyBean(state, this)
    }
}