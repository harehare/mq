" mq.vim - Neovim plugin for mq markdown processing tool
" Maintainer: mq maintainers
" Version: 0.1.0

" Prevent loading the plugin multiple times
if exists('g:loaded_mq_nvim')
  finish
endif
let g:loaded_mq_nvim = 1

" Save user's compatible options
let s:save_cpo = &cpo
set cpo&vim

" Create plugin commands
command! -nargs=0 MqSetup lua require('mq').setup()

" Define configuration variables with defaults
let g:mq_lsp_bin = get(g:, 'mq_lsp_bin', 'mq-lsp')
let g:mq_highlighting = get(g:, 'mq_highlighting', 1)
let g:mq_formatting = get(g:, 'mq_formatting', 1)
let g:mq_snippets = get(g:, 'mq_snippets', 1)

" Initialize the plugin if auto_setup is enabled
if get(g:, 'mq_auto_setup', 1)
  lua << EOF
  require('mq').setup({
    lsp_bin = vim.g.mq_lsp_bin,
    features = {
      highlighting = vim.g.mq_highlighting == 1,
      formatting = vim.g.mq_formatting == 1,
      snippets = vim.g.mq_snippets == 1
    }
  })
EOF
endif

" Restore user's compatible options
let &cpo = s:save_cpo
unlet s:save_cpo
