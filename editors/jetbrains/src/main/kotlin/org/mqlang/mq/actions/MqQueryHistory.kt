package org.mqlang.mq.actions

import com.intellij.ide.util.PropertiesComponent

/** Recently executed `mq: Execute query` queries, most recent first. */
object MqQueryHistory {
    private const val KEY = "mq.queryHistory"
    private const val MAX_SIZE = 20

    fun load(): List<String> =
        PropertiesComponent.getInstance().getValues(KEY)?.toList() ?: emptyList()

    fun add(query: String) {
        if (query.isBlank()) return
        val updated = (listOf(query) + load().filter { it != query }).take(MAX_SIZE)
        PropertiesComponent.getInstance().setValues(KEY, updated.toTypedArray())
    }
}
