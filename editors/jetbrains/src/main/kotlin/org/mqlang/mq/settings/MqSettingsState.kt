package org.mqlang.mq.settings

import com.intellij.openapi.components.PersistentStateComponent
import com.intellij.openapi.components.Service
import com.intellij.openapi.components.State
import com.intellij.openapi.components.Storage
import com.intellij.util.xmlb.XmlSerializerUtil

/** Persisted mq plugin settings, mirroring the mq VS Code extension's `mq.*` configuration keys. */
@Service(Service.Level.APP)
@State(name = "MqSettings", storages = [Storage("mq.xml")])
class MqSettingsState : PersistentStateComponent<MqSettingsState.State> {

    class State {
        var lspPath: String = ""
        var dbgPath: String = ""
        var showExamplesInNewFile: Boolean = true
        var enableTypeCheck: Boolean = false
        var strictArray: Boolean = false
        var enableLint: Boolean = false
        var disabledLintRules: MutableList<String> = mutableListOf()
    }

    private var myState = State()

    override fun getState(): State = myState

    override fun loadState(state: State) {
        XmlSerializerUtil.copyBean(state, myState)
    }

    var lspPath: String
        get() = myState.lspPath
        set(value) {
            myState.lspPath = value
        }

    var dbgPath: String
        get() = myState.dbgPath
        set(value) {
            myState.dbgPath = value
        }

    var showExamplesInNewFile: Boolean
        get() = myState.showExamplesInNewFile
        set(value) {
            myState.showExamplesInNewFile = value
        }

    var enableTypeCheck: Boolean
        get() = myState.enableTypeCheck
        set(value) {
            myState.enableTypeCheck = value
        }

    var strictArray: Boolean
        get() = myState.strictArray
        set(value) {
            myState.strictArray = value
        }

    var enableLint: Boolean
        get() = myState.enableLint
        set(value) {
            myState.enableLint = value
        }

    var disabledLintRules: List<String>
        get() = myState.disabledLintRules
        set(value) {
            myState.disabledLintRules = value.toMutableList()
        }

    companion object {
        @JvmStatic
        fun getInstance(): MqSettingsState =
            com.intellij.openapi.application.ApplicationManager.getApplication().getService(MqSettingsState::class.java)
    }
}
