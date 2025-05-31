" Vim syntax file
" Language: MQ
" Maintainer: Your Name
" Last Change: YYYY MM DD

if exists("b:current_syntax")
  finish
endif

" Keywords
syn keyword mqKeyword if else for let fn return true false null undefined
syn keyword mqStatement import export from async await try catch finally class extends new delete typeof instanceof

" Comments
syn region mqComment start="//" end="$" contains=@Spell
syn region mqCommentBlock start="/\*" end="\*/" contains=@Spell

" Strings
syn region mqString start=/\v"/ skip=/\v\\./ end=/\v"/ contains=@Spell
syn region mqStringSingleQuote start=/\v'/ skip=/\v\\./ end=/\v'/ contains=@Spell
syn region mqTemplateString start=/\v`/ skip=/\v\\./ end=/\v`/ contains=@Spell

" Numbers
syn match mqNumber /\v\<\d+\.?\d*([eE][+-]?\d+)?\>/
syn match mqHexNumber /\v0[xX][0-9a-fA-F]+\>/

" Operators
syn match mqOperator /[+\-*/%=<>!&|?:]/
syn match mqOperator /\v\.\.\./ " Spread/rest operator
syn match mqOperator /\v=>/ " Arrow function

" Braces and Parentheses
syn match mqDelimiter /[(){}\[\]]/

" Highlight links
hi def link mqKeyword Keyword
hi def link mqStatement Statement
hi def link mqComment Comment
hi def link mqCommentBlock Comment
hi def link mqString String
hi def link mqStringSingleQuote String
hi def link mqTemplateString String
hi def link mqNumber Number
hi def link mqHexNumber Number
hi def link mqOperator Operator
hi def link mqDelimiter Delimiter

let b:current_syntax = "mq"

" Optional: Set filetype options
" setlocal comments=://
" setlocal commentstring=//\ %s
" setlocal foldmethod=syntax
