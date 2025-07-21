package org.mqlang.intellij

import com.intellij.psi.tree.IElementType

object MqTypes {
    // Tokens
    @JvmField val COMMENT = MqElementType("COMMENT")
    @JvmField val STRING_LITERAL = MqElementType("STRING_LITERAL")
    @JvmField val NUMBER_LITERAL = MqElementType("NUMBER_LITERAL")
    @JvmField val IDENTIFIER = MqElementType("IDENTIFIER")
    
    // Keywords
    @JvmField val DEF = MqElementType("DEF")
    @JvmField val IF = MqElementType("IF")
    @JvmField val ELSE = MqElementType("ELSE")
    @JvmField val ELIF = MqElementType("ELIF")
    @JvmField val THEN = MqElementType("THEN")
    @JvmField val END = MqElementType("END")
    @JvmField val AND = MqElementType("AND")
    @JvmField val OR = MqElementType("OR")
    @JvmField val NOT = MqElementType("NOT")
    @JvmField val TRY = MqElementType("TRY")
    @JvmField val CATCH = MqElementType("CATCH")
    @JvmField val FOREACH = MqElementType("FOREACH")
    @JvmField val WHILE = MqElementType("WHILE")
    @JvmField val UNTIL = MqElementType("UNTIL")
    @JvmField val IMPORT = MqElementType("IMPORT")
    @JvmField val INCLUDE = MqElementType("INCLUDE")
    @JvmField val MODULE = MqElementType("MODULE")
    @JvmField val AS = MqElementType("AS")
    @JvmField val LET = MqElementType("LET")
    
    // Operators
    @JvmField val PIPE = MqElementType("PIPE")
    @JvmField val PLUS = MqElementType("PLUS")
    @JvmField val MINUS = MqElementType("MINUS")
    @JvmField val MULTIPLY = MqElementType("MULTIPLY")
    @JvmField val DIVIDE = MqElementType("DIVIDE")
    @JvmField val MODULO = MqElementType("MODULO")
    @JvmField val EQUAL = MqElementType("EQUAL")
    @JvmField val NOT_EQUAL = MqElementType("NOT_EQUAL")
    @JvmField val LESS_THAN = MqElementType("LESS_THAN")
    @JvmField val LESS_THAN_OR_EQUAL = MqElementType("LESS_THAN_OR_EQUAL")
    @JvmField val GREATER_THAN = MqElementType("GREATER_THAN")
    @JvmField val GREATER_THAN_OR_EQUAL = MqElementType("GREATER_THAN_OR_EQUAL")
    @JvmField val ASSIGN = MqElementType("ASSIGN")
    
    // Punctuation
    @JvmField val LEFT_PAREN = MqElementType("LEFT_PAREN")
    @JvmField val RIGHT_PAREN = MqElementType("RIGHT_PAREN")
    @JvmField val LEFT_BRACKET = MqElementType("LEFT_BRACKET")
    @JvmField val RIGHT_BRACKET = MqElementType("RIGHT_BRACKET")
    @JvmField val LEFT_BRACE = MqElementType("LEFT_BRACE")
    @JvmField val RIGHT_BRACE = MqElementType("RIGHT_BRACE")
    @JvmField val SEMICOLON = MqElementType("SEMICOLON")
    @JvmField val COLON = MqElementType("COLON")
    @JvmField val COMMA = MqElementType("COMMA")
    @JvmField val DOT = MqElementType("DOT")
    @JvmField val QUESTION = MqElementType("QUESTION")
}

class MqElementType(debugName: String) : IElementType(debugName, MqLanguage)