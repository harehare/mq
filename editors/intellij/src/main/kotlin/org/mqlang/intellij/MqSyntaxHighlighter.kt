package org.mqlang.intellij

import com.intellij.lexer.Lexer
import com.intellij.openapi.editor.DefaultLanguageHighlighterColors
import com.intellij.openapi.editor.colors.TextAttributesKey
import com.intellij.openapi.fileTypes.SyntaxHighlighterBase
import com.intellij.psi.tree.IElementType

class MqSyntaxHighlighter : SyntaxHighlighterBase() {
    
    companion object {
        val KEYWORD = TextAttributesKey.createTextAttributesKey(
            "MQ_KEYWORD", DefaultLanguageHighlighterColors.KEYWORD
        )
        val STRING = TextAttributesKey.createTextAttributesKey(
            "MQ_STRING", DefaultLanguageHighlighterColors.STRING
        )
        val NUMBER = TextAttributesKey.createTextAttributesKey(
            "MQ_NUMBER", DefaultLanguageHighlighterColors.NUMBER
        )
        val COMMENT = TextAttributesKey.createTextAttributesKey(
            "MQ_COMMENT", DefaultLanguageHighlighterColors.LINE_COMMENT
        )
        val FUNCTION = TextAttributesKey.createTextAttributesKey(
            "MQ_FUNCTION", DefaultLanguageHighlighterColors.FUNCTION_CALL
        )
        val OPERATOR = TextAttributesKey.createTextAttributesKey(
            "MQ_OPERATOR", DefaultLanguageHighlighterColors.OPERATION_SIGN
        )
        val PARENTHESES = TextAttributesKey.createTextAttributesKey(
            "MQ_PARENTHESES", DefaultLanguageHighlighterColors.PARENTHESES
        )
        val BRACKETS = TextAttributesKey.createTextAttributesKey(
            "MQ_BRACKETS", DefaultLanguageHighlighterColors.BRACKETS
        )
        val BRACES = TextAttributesKey.createTextAttributesKey(
            "MQ_BRACES", DefaultLanguageHighlighterColors.BRACES
        )
        val DOT = TextAttributesKey.createTextAttributesKey(
            "MQ_DOT", DefaultLanguageHighlighterColors.DOT
        )
    }
    
    override fun getHighlightingLexer(): Lexer {
        return MqLexerAdapter()
    }
    
    override fun getTokenHighlights(tokenType: IElementType?): Array<TextAttributesKey> {
        return when (tokenType) {
            MqTypes.COMMENT -> arrayOf(COMMENT)
            MqTypes.STRING_LITERAL -> arrayOf(STRING)
            MqTypes.NUMBER_LITERAL -> arrayOf(NUMBER)
            MqTypes.DEF, MqTypes.IF, MqTypes.ELSE, MqTypes.ELIF, MqTypes.THEN, MqTypes.END,
            MqTypes.AND, MqTypes.OR, MqTypes.NOT, MqTypes.TRY, MqTypes.CATCH,
            MqTypes.FOREACH, MqTypes.WHILE, MqTypes.UNTIL, MqTypes.IMPORT,
            MqTypes.INCLUDE, MqTypes.MODULE, MqTypes.AS, MqTypes.LET -> arrayOf(KEYWORD)
            MqTypes.PIPE -> arrayOf(OPERATOR)
            MqTypes.PLUS, MqTypes.MINUS, MqTypes.MULTIPLY, MqTypes.DIVIDE, MqTypes.MODULO,
            MqTypes.EQUAL, MqTypes.NOT_EQUAL, MqTypes.LESS_THAN, MqTypes.LESS_THAN_OR_EQUAL,
            MqTypes.GREATER_THAN, MqTypes.GREATER_THAN_OR_EQUAL, MqTypes.ASSIGN -> arrayOf(OPERATOR)
            MqTypes.LEFT_PAREN, MqTypes.RIGHT_PAREN -> arrayOf(PARENTHESES)
            MqTypes.LEFT_BRACKET, MqTypes.RIGHT_BRACKET -> arrayOf(BRACKETS)
            MqTypes.LEFT_BRACE, MqTypes.RIGHT_BRACE -> arrayOf(BRACES)
            MqTypes.DOT -> arrayOf(DOT)
            MqTypes.IDENTIFIER -> {
                // Check if it's a built-in function
                arrayOf()
            }
            else -> arrayOf()
        }
    }
}