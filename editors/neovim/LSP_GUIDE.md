# mq.nvim LSP Setup Guide

このガイドでは、mq の Language Server Protocol (LSP) を Neovim で使用する方法を説明します。

## 前提条件

### 1. mq コマンドのインストール

LSP機能を使用するには、`mq` コマンドがインストールされている必要があります。

```bash
# mq がインストールされているか確認
which mq

# mq のバージョンを確認
mq --version
```

インストールされていない場合：

```bash
# Cargo でインストール
cargo install --git https://github.com/harehare/mq.git mq-run

# または Neovim 内から
:MqInstallServers
```

### 2. mq.nvim のセットアップ

`~/.config/nvim/init.lua` に以下を追加：

```lua
-- mq.nvim を runtimepath に追加
vim.opt.runtimepath:append(vim.fn.expand("~/git/mq/editors/neovim"))

-- mq.nvim をセットアップ
require("mq").setup({
  cmd = "mq",  -- mq コマンドのパス（PATH にない場合は絶対パスを指定）
  auto_start_lsp = true,  -- .mq ファイルを開いた時に自動で LSP を起動
  lsp = {
    on_attach = function(client, bufnr)
      -- LSP が起動したときの通知
      print("mq LSP attached to buffer " .. bufnr)

      -- キーマップの設定
      local opts = { buffer = bufnr, noremap = true, silent = true }

      -- 定義へジャンプ
      vim.keymap.set("n", "gd", vim.lsp.buf.definition, opts)

      -- ホバー情報を表示
      vim.keymap.set("n", "K", vim.lsp.buf.hover, opts)

      -- 実装へジャンプ
      vim.keymap.set("n", "gi", vim.lsp.buf.implementation, opts)

      -- シグネチャヘルプ
      vim.keymap.set("n", "<C-k>", vim.lsp.buf.signature_help, opts)

      -- リネーム
      vim.keymap.set("n", "<leader>rn", vim.lsp.buf.rename, opts)

      -- コードアクション
      vim.keymap.set("n", "<leader>ca", vim.lsp.buf.code_action, opts)

      -- 参照を表示
      vim.keymap.set("n", "gr", vim.lsp.buf.references, opts)

      -- フォーマット
      vim.keymap.set("n", "<leader>f", function()
        vim.lsp.buf.format({ async = true })
      end, opts)
    end,

    -- 補完機能の設定（nvim-cmp を使用している場合）
    capabilities = (function()
      local has_cmp, cmp_nvim_lsp = pcall(require, "cmp_nvim_lsp")
      if has_cmp then
        return cmp_nvim_lsp.default_capabilities()
      end
      return vim.lsp.protocol.make_client_capabilities()
    end)(),
  },
})
```

## LSP の使用方法

### 自動起動

設定で `auto_start_lsp = true` にしている場合、`.mq` ファイルを開くと自動的に LSP サーバーが起動します。

```bash
# テスト用の .mq ファイルを作成
echo '.code("js")' > test.mq

# Neovim で開く（LSP が自動起動）
nvim test.mq
```

### 手動起動

```vim
" LSP サーバーを起動
:MqStartLSP

" LSP サーバーを停止
:MqStopLSP

" LSP サーバーを再起動
:MqRestartLSP
```

### LSP の状態を確認

```vim
" LSP の状態を確認
:LspInfo

" 出力例:
" Language client log: /Users/username/.local/state/nvim/lsp.log
" Detected filetype:   mq
"
" 1 client(s) attached to this buffer:
"
" Client: mq (id: 1, bufnr: [1])
"   filetypes:       mq
"   autostart:       false
"   root directory:  /Users/username/projects/mq
"   cmd:             mq lsp -M /Users/username/projects/mq
```

### LSP ログを確認

```vim
" LSP ログファイルのパスを表示
:lua print(vim.lsp.get_log_path())

" ログを開く
:lua vim.cmd('e ' .. vim.lsp.get_log_path())
```

## LSP 機能

### 1. コード補完

`.mq` ファイルを編集中に、関数名やキーワードの補完が表示されます。

```mq
# 'up' と入力すると 'upcase' が補完候補に表示される
up<Ctrl-n>
```

### 2. ホバー情報 (K)

関数名やキーワードにカーソルを合わせて `K` を押すと、ドキュメントが表示されます。

```mq
upcase("hello")
# ↑ 'upcase' にカーソルを合わせて K を押す
```

### 3. 定義へジャンプ (gd)

関数定義にジャンプできます。

```mq
def my_function(x):
  upcase(x);

my_function("test")
# ↑ 'my_function' にカーソルを合わせて gd を押すと定義にジャンプ
```

### 4. 診断 (エラーチェック)

構文エラーや型エラーがリアルタイムで表示されます。

```mq
# 構文エラーの例
let x =
# ↑ エラーが表示される
```

診断を確認：

```vim
" 診断リストを開く
:lua vim.diagnostic.setloclist()

" 次のエラーへ移動
]d

" 前のエラーへ移動
[d

" カーソル位置の診断を表示
:lua vim.diagnostic.open_float()
```

### 5. コードアクション (<leader>ca)

利用可能なコードアクションを表示します。

### 6. リネーム (<leader>rn)

変数名や関数名を一括で変更できます。

```mq
def old_name(x):
  x;

old_name("test")
# ↑ 'old_name' にカーソルを合わせて <leader>rn を押して新しい名前を入力
```

## トラブルシューティング

### LSP が起動しない

**1. mq コマンドが利用可能か確認**

```bash
which mq
# 出力: /path/to/mq または何も表示されない
```

出力がない場合は mq をインストール：

```bash
cargo install --git https://github.com/harehare/mq.git mq-run
```

**2. LSP サーバーを手動でテスト**

```bash
# LSP サーバーを直接起動してテスト
mq lsp

# 正常に起動すれば、JSON-RPC メッセージを待つ状態になる
# Ctrl-C で停止
```

**3. Neovim で LSP の状態を確認**

```vim
:LspInfo
```

クライアントが表示されない場合：

```vim
" LSP を手動で起動
:MqStartLSP

" 再度確認
:LspInfo
```

**4. ログを確認**

```vim
:lua vim.cmd('e ' .. vim.lsp.get_log_path())
```

エラーメッセージを確認してください。

### LSP は起動するが機能しない

**1. capabilities の設定を確認**

`nvim-cmp` を使用している場合、capabilities を正しく設定する必要があります：

```lua
require("mq").setup({
  lsp = {
    capabilities = require("cmp_nvim_lsp").default_capabilities(),
  },
})
```

**2. on_attach が呼ばれているか確認**

```lua
require("mq").setup({
  lsp = {
    on_attach = function(client, bufnr)
      print("LSP attached! Client: " .. client.name .. ", Buffer: " .. bufnr)
    end,
  },
})
```

**3. LSP サーバーのバージョンを確認**

```bash
mq --version
```

古いバージョンの場合は更新：

```bash
cargo install --git https://github.com/harehare/mq.git mq-run --force
```

### 特定の機能が動かない

**補完が表示されない:**

- `nvim-cmp` などの補完プラグインがインストールされているか確認
- `capabilities` が正しく設定されているか確認

**ホバー (K) が動かない:**

- LSP サーバーが正しく起動しているか `:LspInfo` で確認
- `:lua vim.lsp.buf.hover()` を直接実行してテスト

**診断が表示されない:**

- `:lua vim.diagnostic.enable()` で診断を有効化
- `:lua print(vim.inspect(vim.diagnostic.get()))` で診断情報を確認

## 推奨プラグイン

LSP をより快適に使うための推奨プラグイン：

### 1. nvim-lspconfig

LSP の設定を簡素化（mq.nvim は内部で独自に LSP を起動するため不要ですが、他の言語で使用）

### 2. nvim-cmp

補完機能を提供

```lua
-- lazy.nvim での設定例
{
  "hrsh7th/nvim-cmp",
  dependencies = {
    "hrsh7th/cmp-nvim-lsp",
    "hrsh7th/cmp-buffer",
    "hrsh7th/cmp-path",
  },
  config = function()
    local cmp = require("cmp")
    cmp.setup({
      sources = {
        { name = "nvim_lsp" },
        { name = "buffer" },
        { name = "path" },
      },
      mapping = cmp.mapping.preset.insert({
        ["<C-Space>"] = cmp.mapping.complete(),
        ["<CR>"] = cmp.mapping.confirm({ select = true }),
      }),
    })
  end,
}
```

### 3. telescope.nvim

定義や参照の検索を快適に

```lua
{
  "nvim-telescope/telescope.nvim",
  config = function()
    local builtin = require("telescope.builtin")
    vim.keymap.set("n", "gr", builtin.lsp_references)
    vim.keymap.set("n", "<leader>ds", builtin.lsp_document_symbols)
  end,
}
```

### 4. trouble.nvim

診断情報を見やすく表示

```lua
{
  "folke/trouble.nvim",
  config = function()
    require("trouble").setup()
    vim.keymap.set("n", "<leader>xx", "<cmd>Trouble<cr>")
  end,
}
```

## 完全な設定例

```lua
-- ~/.config/nvim/init.lua

-- lazy.nvim setup
local lazypath = vim.fn.stdpath("data") .. "/lazy/lazy.nvim"
if not vim.loop.fs_stat(lazypath) then
  vim.fn.system({
    "git", "clone", "--filter=blob:none",
    "https://github.com/folke/lazy.nvim.git",
    "--branch=stable", lazypath,
  })
end
vim.opt.rtp:prepend(lazypath)

require("lazy").setup({
  -- mq.nvim
  {
    "mq.nvim",
    dir = vim.fn.expand("~/git/mq/editors/neovim"),
    config = function()
      require("mq").setup({
        auto_start_lsp = true,
        lsp = {
          on_attach = function(client, bufnr)
            local opts = { buffer = bufnr }
            vim.keymap.set("n", "gd", vim.lsp.buf.definition, opts)
            vim.keymap.set("n", "K", vim.lsp.buf.hover, opts)
            vim.keymap.set("n", "gr", vim.lsp.buf.references, opts)
            vim.keymap.set("n", "<leader>rn", vim.lsp.buf.rename, opts)
            vim.keymap.set("n", "<leader>ca", vim.lsp.buf.code_action, opts)
          end,
          capabilities = require("cmp_nvim_lsp").default_capabilities(),
        },
      })
    end,
  },

  -- 補完
  {
    "hrsh7th/nvim-cmp",
    dependencies = {
      "hrsh7th/cmp-nvim-lsp",
      "hrsh7th/cmp-buffer",
    },
    config = function()
      local cmp = require("cmp")
      cmp.setup({
        sources = {
          { name = "nvim_lsp" },
          { name = "buffer" },
        },
        mapping = cmp.mapping.preset.insert({
          ["<C-Space>"] = cmp.mapping.complete(),
          ["<CR>"] = cmp.mapping.confirm({ select = true }),
        }),
      })
    end,
  },
})

-- 診断の設定
vim.diagnostic.config({
  virtual_text = true,
  signs = true,
  update_in_insert = false,
})

-- 診断のキーマップ
vim.keymap.set("n", "[d", vim.diagnostic.goto_prev)
vim.keymap.set("n", "]d", vim.diagnostic.goto_next)
vim.keymap.set("n", "<leader>e", vim.diagnostic.open_float)
vim.keymap.set("n", "<leader>q", vim.diagnostic.setloclist)
```

## 参考リンク

- [Neovim LSP ドキュメント](https://neovim.io/doc/user/lsp.html)
- [mq 公式サイト](https://mqlang.org/)
- [mq GitHub リポジトリ](https://github.com/harehare/mq)
