package org.mqlang.intellij

import com.intellij.lang.ASTNode
import com.intellij.psi.PsiElement
import com.intellij.psi.impl.source.tree.LeafPsiElement

object MqPsiElementFactory {
    fun createElement(node: ASTNode?): PsiElement {
        return LeafPsiElement(node!!.elementType, node.text)
    }
}