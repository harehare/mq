" ftplugin for mq: matchit/matchparen support for block keywords
if exists("b:did_ftplugin")
  finish
endif
let b:did_ftplugin = 1

if !exists("g:loaded_matchit") && findfile("macros/matchit.vim", &runtimepath) !=# ""
  runtime! macros/matchit.vim
endif

let b:match_words = '\<\%(def\|fn\|while\|until\|loop\|foreach\|match\|module\)\>:\<end\>'

" Ignore keywords found inside comments or strings.
let b:match_skip = 's:mqComment\|mqString\|mqStringBytes\|mqStringInterpolate'

let b:undo_ftplugin = "unlet! b:match_words b:match_skip"
