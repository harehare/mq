package org.mqlang.intellij

import com.intellij.psi.tree.TokenSet

object MqTokenSets {
    val COMMENTS = TokenSet.create(MqTypes.COMMENT)
    val STRINGS = TokenSet.create(MqTypes.STRING_LITERAL)
    val NUMBERS = TokenSet.create(MqTypes.NUMBER_LITERAL)
    val KEYWORDS = TokenSet.create(
        MqTypes.DEF, MqTypes.IF, MqTypes.ELSE, MqTypes.ELIF, MqTypes.THEN, MqTypes.END,
        MqTypes.AND, MqTypes.OR, MqTypes.NOT, MqTypes.TRY, MqTypes.CATCH,
        MqTypes.FOREACH, MqTypes.WHILE, MqTypes.UNTIL, MqTypes.IMPORT,
        MqTypes.INCLUDE, MqTypes.MODULE, MqTypes.AS, MqTypes.LET
    )
    val OPERATORS = TokenSet.create(
        MqTypes.PIPE, MqTypes.PLUS, MqTypes.MINUS, MqTypes.MULTIPLY, MqTypes.DIVIDE,
        MqTypes.MODULO, MqTypes.EQUAL, MqTypes.NOT_EQUAL, MqTypes.LESS_THAN,
        MqTypes.LESS_THAN_OR_EQUAL, MqTypes.GREATER_THAN, MqTypes.GREATER_THAN_OR_EQUAL,
        MqTypes.ASSIGN
    )
}