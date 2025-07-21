package org.mqlang.intellij

import com.intellij.lexer.FlexAdapter

class MqLexerAdapter : FlexAdapter(MqLexer(null))