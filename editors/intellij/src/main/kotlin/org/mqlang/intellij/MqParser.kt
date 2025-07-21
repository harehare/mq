package org.mqlang.intellij

import com.intellij.lang.ASTNode
import com.intellij.lang.PsiBuilder
import com.intellij.lang.PsiParser
import com.intellij.psi.tree.IElementType

class MqParser : PsiParser {
    
    override fun parse(root: IElementType, builder: PsiBuilder): ASTNode {
        val rootMarker = builder.mark()
        
        while (!builder.eof()) {
            parseExpression(builder)
        }
        
        rootMarker.done(root)
        return builder.treeBuilt
    }
    
    private fun parseExpression(builder: PsiBuilder) {
        val tokenType = builder.tokenType
        
        when (tokenType) {
            MqTypes.COMMENT, 
            MqTypes.STRING_LITERAL,
            MqTypes.NUMBER_LITERAL,
            MqTypes.IDENTIFIER -> builder.advanceLexer()
            
            MqTypes.DEF -> parseDefExpression(builder)
            MqTypes.IF -> parseIfExpression(builder)
            MqTypes.LET -> parseLetExpression(builder)
            
            else -> builder.advanceLexer()
        }
    }
    
    private fun parseDefExpression(builder: PsiBuilder) {
        val marker = builder.mark()
        builder.advanceLexer() // consume 'def'
        
        // Parse function name
        if (builder.tokenType == MqTypes.IDENTIFIER) {
            builder.advanceLexer()
        }
        
        // Parse parameters if present
        if (builder.tokenType == MqTypes.LEFT_PAREN) {
            builder.advanceLexer()
            while (builder.tokenType != MqTypes.RIGHT_PAREN && !builder.eof()) {
                builder.advanceLexer()
            }
            if (builder.tokenType == MqTypes.RIGHT_PAREN) {
                builder.advanceLexer()
            }
        }
        
        // Parse colon
        if (builder.tokenType == MqTypes.COLON) {
            builder.advanceLexer()
        }
        
        // Parse body
        while (builder.tokenType != MqTypes.END && !builder.eof()) {
            parseExpression(builder)
        }
        
        if (builder.tokenType == MqTypes.END) {
            builder.advanceLexer()
        }
        
        marker.done(MqTypes.DEF)
    }
    
    private fun parseIfExpression(builder: PsiBuilder) {
        val marker = builder.mark()
        builder.advanceLexer() // consume 'if'
        
        // Parse condition
        parseExpression(builder)
        
        // Parse then
        if (builder.tokenType == MqTypes.THEN) {
            builder.advanceLexer()
        }
        
        // Parse body
        while (builder.tokenType !in listOf(MqTypes.ELSE, MqTypes.ELIF, MqTypes.END) && !builder.eof()) {
            parseExpression(builder)
        }
        
        // Parse else/elif clauses
        while (builder.tokenType in listOf(MqTypes.ELSE, MqTypes.ELIF) && !builder.eof()) {
            builder.advanceLexer()
            if (builder.tokenType == MqTypes.IF) {
                parseExpression(builder)
            }
            if (builder.tokenType == MqTypes.THEN) {
                builder.advanceLexer()
            }
            while (builder.tokenType !in listOf(MqTypes.ELSE, MqTypes.ELIF, MqTypes.END) && !builder.eof()) {
                parseExpression(builder)
            }
        }
        
        if (builder.tokenType == MqTypes.END) {
            builder.advanceLexer()
        }
        
        marker.done(MqTypes.IF)
    }
    
    private fun parseLetExpression(builder: PsiBuilder) {
        val marker = builder.mark()
        builder.advanceLexer() // consume 'let'
        
        // Parse variable name
        if (builder.tokenType == MqTypes.IDENTIFIER) {
            builder.advanceLexer()
        }
        
        // Parse assignment
        if (builder.tokenType == MqTypes.ASSIGN) {
            builder.advanceLexer()
            parseExpression(builder)
        }
        
        marker.done(MqTypes.LET)
    }
}