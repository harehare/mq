-- File type detection for mq files
vim.cmd([[
  augroup mq_ftdetect
    autocmd!
    autocmd BufNewFile,BufRead *.mq setfiletype mq
  augroup END
]])
