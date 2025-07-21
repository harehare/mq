package org.mqlang.intellij

import com.intellij.lexer.LexerBase
import com.intellij.psi.tree.IElementType

class MqLexer(buffer: CharSequence?) : LexerBase() {
    private var myBuffer: CharSequence? = null
    private var myBufferEnd: Int = 0
    private var myTokenStart: Int = 0
    private var myTokenEnd: Int = 0
    private var myCurrentPosition: Int = 0
    private var myTokenType: IElementType? = null

    init {
        start(buffer ?: "", 0, buffer?.length ?: 0, 0)
    }

    override fun start(buffer: CharSequence, startOffset: Int, endOffset: Int, initialState: Int) {
        myBuffer = buffer
        myBufferEnd = endOffset
        myCurrentPosition = startOffset
        myTokenStart = startOffset
        myTokenEnd = startOffset
        myTokenType = null
        advance()
    }

    override fun getState(): Int = 0

    override fun getTokenType(): IElementType? = myTokenType

    override fun getTokenStart(): Int = myTokenStart

    override fun getTokenEnd(): Int = myTokenEnd

    override fun advance() {
        myTokenStart = myCurrentPosition
        if (myCurrentPosition >= myBufferEnd) {
            myTokenType = null
            return
        }

        val ch = myBuffer!![myCurrentPosition]
        when {
            ch.isWhitespace() -> {
                skipWhitespace()
                myTokenType = com.intellij.psi.TokenType.WHITE_SPACE
            }
            ch == '#' -> {
                skipComment()
                myTokenType = MqTypes.COMMENT
            }
            ch == '"' -> {
                skipString()
                myTokenType = MqTypes.STRING_LITERAL
            }
            ch.isDigit() -> {
                skipNumber()
                myTokenType = MqTypes.NUMBER_LITERAL
            }
            ch.isLetter() || ch == '_' -> {
                skipIdentifier()
                val text = myBuffer!!.subSequence(myTokenStart, myCurrentPosition).toString()
                myTokenType = getKeywordType(text)
            }
            ch == '|' -> {
                myCurrentPosition++
                myTokenType = MqTypes.PIPE
            }
            ch == '(' -> {
                myCurrentPosition++
                myTokenType = MqTypes.LEFT_PAREN
            }
            ch == ')' -> {
                myCurrentPosition++
                myTokenType = MqTypes.RIGHT_PAREN
            }
            ch == '[' -> {
                myCurrentPosition++
                myTokenType = MqTypes.LEFT_BRACKET
            }
            ch == ']' -> {
                myCurrentPosition++
                myTokenType = MqTypes.RIGHT_BRACKET
            }
            ch == '{' -> {
                myCurrentPosition++
                myTokenType = MqTypes.LEFT_BRACE
            }
            ch == '}' -> {
                myCurrentPosition++
                myTokenType = MqTypes.RIGHT_BRACE
            }
            ch == '.' -> {
                myCurrentPosition++
                myTokenType = MqTypes.DOT
            }
            ch == '+' -> {
                myCurrentPosition++
                myTokenType = MqTypes.PLUS
            }
            ch == '-' -> {
                myCurrentPosition++
                myTokenType = MqTypes.MINUS
            }
            ch == '*' -> {
                myCurrentPosition++
                myTokenType = MqTypes.MULTIPLY
            }
            ch == '/' -> {
                myCurrentPosition++
                myTokenType = MqTypes.DIVIDE
            }
            ch == '%' -> {
                myCurrentPosition++
                myTokenType = MqTypes.MODULO
            }
            ch == '=' -> {
                myCurrentPosition++
                if (myCurrentPosition < myBufferEnd && myBuffer!![myCurrentPosition] == '=') {
                    myCurrentPosition++
                    myTokenType = MqTypes.EQUAL
                } else {
                    myTokenType = MqTypes.ASSIGN
                }
            }
            ch == '!' -> {
                myCurrentPosition++
                if (myCurrentPosition < myBufferEnd && myBuffer!![myCurrentPosition] == '=') {
                    myCurrentPosition++
                    myTokenType = MqTypes.NOT_EQUAL
                } else {
                    myCurrentPosition--
                    skipIdentifier()
                    myTokenType = MqTypes.IDENTIFIER
                }
            }
            ch == '<' -> {
                myCurrentPosition++
                if (myCurrentPosition < myBufferEnd && myBuffer!![myCurrentPosition] == '=') {
                    myCurrentPosition++
                    myTokenType = MqTypes.LESS_THAN_OR_EQUAL
                } else {
                    myTokenType = MqTypes.LESS_THAN
                }
            }
            ch == '>' -> {
                myCurrentPosition++
                if (myCurrentPosition < myBufferEnd && myBuffer!![myCurrentPosition] == '=') {
                    myCurrentPosition++
                    myTokenType = MqTypes.GREATER_THAN_OR_EQUAL
                } else {
                    myTokenType = MqTypes.GREATER_THAN
                }
            }
            ch == ';' -> {
                myCurrentPosition++
                myTokenType = MqTypes.SEMICOLON
            }
            ch == ':' -> {
                myCurrentPosition++
                myTokenType = MqTypes.COLON
            }
            ch == ',' -> {
                myCurrentPosition++
                myTokenType = MqTypes.COMMA
            }
            ch == '?' -> {
                myCurrentPosition++
                myTokenType = MqTypes.QUESTION
            }
            else -> {
                myCurrentPosition++
                myTokenType = com.intellij.psi.TokenType.BAD_CHARACTER
            }
        }
        myTokenEnd = myCurrentPosition
    }

    private fun skipWhitespace() {
        while (myCurrentPosition < myBufferEnd && myBuffer!![myCurrentPosition].isWhitespace()) {
            myCurrentPosition++
        }
    }

    private fun skipComment() {
        while (myCurrentPosition < myBufferEnd && myBuffer!![myCurrentPosition] != '\n') {
            myCurrentPosition++
        }
    }

    private fun skipString() {
        myCurrentPosition++ // skip opening quote
        while (myCurrentPosition < myBufferEnd) {
            val ch = myBuffer!![myCurrentPosition]
            if (ch == '"') {
                myCurrentPosition++ // skip closing quote
                break
            }
            if (ch == '\\' && myCurrentPosition + 1 < myBufferEnd) {
                myCurrentPosition++ // skip escape character
            }
            myCurrentPosition++
        }
    }

    private fun skipNumber() {
        while (myCurrentPosition < myBufferEnd && (myBuffer!![myCurrentPosition].isDigit() || myBuffer!![myCurrentPosition] == '.')) {
            myCurrentPosition++
        }
    }

    private fun skipIdentifier() {
        while (myCurrentPosition < myBufferEnd && (myBuffer!![myCurrentPosition].isLetterOrDigit() || myBuffer!![myCurrentPosition] == '_')) {
            myCurrentPosition++
        }
    }

    private fun getKeywordType(text: String): IElementType {
        return when (text) {
            "def" -> MqTypes.DEF
            "if" -> MqTypes.IF
            "else" -> MqTypes.ELSE
            "elif" -> MqTypes.ELIF
            "then" -> MqTypes.THEN
            "end" -> MqTypes.END
            "and" -> MqTypes.AND
            "or" -> MqTypes.OR
            "not" -> MqTypes.NOT
            "try" -> MqTypes.TRY
            "catch" -> MqTypes.CATCH
            "foreach" -> MqTypes.FOREACH
            "while" -> MqTypes.WHILE
            "until" -> MqTypes.UNTIL
            "import" -> MqTypes.IMPORT
            "include" -> MqTypes.INCLUDE
            "module" -> MqTypes.MODULE
            "as" -> MqTypes.AS
            "let" -> MqTypes.LET
            else -> MqTypes.IDENTIFIER
        }
    }

    override fun getBufferSequence(): CharSequence = myBuffer!!

    override fun getBufferEnd(): Int = myBufferEnd
}