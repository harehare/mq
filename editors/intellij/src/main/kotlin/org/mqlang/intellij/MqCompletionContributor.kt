package org.mqlang.intellij

import com.intellij.codeInsight.completion.*
import com.intellij.codeInsight.lookup.LookupElementBuilder
import com.intellij.patterns.PlatformPatterns
import com.intellij.util.ProcessingContext

class MqCompletionContributor : CompletionContributor() {
    
    init {
        extend(
            CompletionType.BASIC,
            PlatformPatterns.psiElement().withLanguage(MqLanguage),
            MqCompletionProvider()
        )
    }
    
    private class MqCompletionProvider : CompletionProvider<CompletionParameters>() {
        
        override fun addCompletions(
            parameters: CompletionParameters,
            context: ProcessingContext,
            result: CompletionResultSet
        ) {
            // Built-in functions
            val builtinFunctions = listOf(
                "add", "sub", "mul", "div", "mod", "abs", "floor", "ceil", "round",
                "length", "keys", "values", "empty", "error", "type", "select",
                "map", "sort", "sort_by", "group_by", "unique", "unique_by",
                "reverse", "flatten", "min", "max", "min_by", "max_by",
                "has", "in", "contains", "startswith", "endswith", "ltrimstr",
                "rtrimstr", "split", "join", "ascii_downcase", "ascii_upcase",
                "to_number", "tostring", "tonumber", "todate", "now",
                "first", "last", "nth", "range", "repeat", "while", "until",
                "if", "try", "catch", "reduce", "foreach", "recurse",
                "walk", "transpose", "combinations", "limit", "from_entries",
                "to_entries", "with_entries", "paths", "leaf_paths", "any",
                "all", "not", "is_null", "is_number", "is_string", "is_array",
                "is_object", "is_boolean", "is_empty", "is_finite", "is_infinite",
                "is_nan", "is_normal", "is_mdx", "to_text", "to_link", "to_md_list",
                "upcase", "downcase"
            )
            
            builtinFunctions.forEach { function ->
                result.addElement(
                    LookupElementBuilder.create(function)
                        .withTypeText("function")
                        .withIcon(MqIcons.FILE)
                )
            }
            
            // Keywords
            val keywords = listOf(
                "def", "if", "else", "elif", "then", "end", "and", "or", "not",
                "try", "catch", "foreach", "while", "until", "import", "include",
                "module", "as", "let"
            )
            
            keywords.forEach { keyword ->
                result.addElement(
                    LookupElementBuilder.create(keyword)
                        .withTypeText("keyword")
                        .bold()
                )
            }
            
            // Common selectors
            val selectors = listOf(
                ".[]", ".h", ".h1", ".h2", ".h3", ".h4", ".h5", ".h6",
                ".p", ".code", ".blockquote", ".list", ".link", ".image",
                ".table", ".th", ".td", ".tr"
            )
            
            selectors.forEach { selector ->
                result.addElement(
                    LookupElementBuilder.create(selector)
                        .withTypeText("selector")
                )
            }
        }
    }
}